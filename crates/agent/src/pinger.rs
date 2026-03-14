use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use serverbee_common::constants::{has_capability, probe_type_to_cap};
use serverbee_common::types::{PingResult, PingTaskConfig};
use tokio::sync::mpsc;

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

        let result = match config.probe_type.as_str() {
            "icmp" => probe_icmp(&config.task_id, &config.target).await,
            "tcp" => probe_tcp(&config.task_id, &config.target).await,
            "http" => probe_http(&config.task_id, &config.target).await,
            other => PingResult {
                task_id: config.task_id.clone(),
                latency: 0.0,
                success: false,
                error: Some(format!("Unknown probe type: {other}")),
                time: Utc::now(),
            },
        };

        if tx.send(result).await.is_err() {
            tracing::debug!("Ping result channel closed, stopping task {}", config.task_id);
            return;
        }
    }
}

async fn probe_icmp(task_id: &str, target: &str) -> PingResult {
    let start = Instant::now();

    // Use system ping command — works without root on most systems
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new("ping")
            .args(["-c", "1", "-W", "5", target])
            .output(),
    )
    .await;

    match output {
        Ok(Ok(out)) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if out.status.success() {
                // Try to parse RTT from output (e.g. "time=1.23 ms")
                let stdout = String::from_utf8_lossy(&out.stdout);
                let latency = parse_ping_time(&stdout).unwrap_or(elapsed);
                PingResult {
                    task_id: task_id.to_string(),
                    latency,
                    success: true,
                    error: None,
                    time: Utc::now(),
                }
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                PingResult {
                    task_id: task_id.to_string(),
                    latency: 0.0,
                    success: false,
                    error: Some(format!("Ping failed: {}", stderr.trim())),
                    time: Utc::now(),
                }
            }
        }
        Ok(Err(e)) => PingResult {
            task_id: task_id.to_string(),
            latency: 0.0,
            success: false,
            error: Some(format!("Failed to run ping: {e}")),
            time: Utc::now(),
        },
        Err(_) => PingResult {
            task_id: task_id.to_string(),
            latency: 0.0,
            success: false,
            error: Some("Ping timed out".to_string()),
            time: Utc::now(),
        },
    }
}

async fn probe_tcp(task_id: &str, target: &str) -> PingResult {
    let start = Instant::now();

    // Target should be host:port
    let addr = if target.contains(':') {
        target.to_string()
    } else {
        format!("{target}:80")
    };

    let result = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::TcpStream::connect(&addr),
    )
    .await;

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(Ok(_stream)) => PingResult {
            task_id: task_id.to_string(),
            latency: elapsed,
            success: true,
            error: None,
            time: Utc::now(),
        },
        Ok(Err(e)) => PingResult {
            task_id: task_id.to_string(),
            latency: 0.0,
            success: false,
            error: Some(format!("TCP connect failed: {e}")),
            time: Utc::now(),
        },
        Err(_) => PingResult {
            task_id: task_id.to_string(),
            latency: 0.0,
            success: false,
            error: Some("TCP connect timed out".to_string()),
            time: Utc::now(),
        },
    }
}

async fn probe_http(task_id: &str, target: &str) -> PingResult {
    let start = Instant::now();

    let url = if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("http://{target}")
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return PingResult {
                task_id: task_id.to_string(),
                latency: 0.0,
                success: false,
                error: Some(format!("Failed to create HTTP client: {e}")),
                time: Utc::now(),
            };
        }
    };

    match client.get(&url).send().await {
        Ok(resp) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let status = resp.status();
            if status.is_success() || status.is_redirection() {
                PingResult {
                    task_id: task_id.to_string(),
                    latency: elapsed,
                    success: true,
                    error: None,
                    time: Utc::now(),
                }
            } else {
                PingResult {
                    task_id: task_id.to_string(),
                    latency: elapsed,
                    success: false,
                    error: Some(format!("HTTP {status}")),
                    time: Utc::now(),
                }
            }
        }
        Err(e) => PingResult {
            task_id: task_id.to_string(),
            latency: 0.0,
            success: false,
            error: Some(format!("HTTP request failed: {e}")),
            time: Utc::now(),
        },
    }
}

/// Parse "time=X.XX ms" from ping output
fn parse_ping_time(output: &str) -> Option<f64> {
    for line in output.lines() {
        if let Some(pos) = line.find("time=") {
            let rest = &line[pos + 5..];
            let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            if let Ok(ms) = num_str.parse::<f64>() {
                return Some(ms);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_ping_open_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target = format!("127.0.0.1:{}", addr.port());
        let result = probe_tcp("test-task", &target).await;
        assert!(result.success);
        assert!(result.latency > 0.0);
    }

    #[tokio::test]
    async fn test_tcp_ping_closed_port() {
        // Port 1 is reserved and should be closed/refused on loopback
        let result = probe_tcp("test-task", "127.0.0.1:1").await;
        assert!(!result.success);
    }
}
