use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serverbee_common::constants::{DEFAULT_COMMAND_TIMEOUT_SECS, DEFAULT_REPORT_INTERVAL, MAX_TASK_OUTPUT_SIZE};
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::time::interval;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::collector::Collector;
use crate::config::AgentConfig;

const MAX_BACKOFF_SECS: u64 = 30;
const JITTER_FACTOR: f64 = 0.2;

pub struct Reporter {
    config: AgentConfig,
}

impl Reporter {
    pub fn new(config: AgentConfig) -> Self {
        Self { config }
    }

    pub async fn run(&mut self) {
        let mut backoff_secs: u64 = 1;

        loop {
            match self.connect_and_report().await {
                Ok(()) => {
                    tracing::info!("Connection closed normally, reconnecting...");
                    backoff_secs = 1;
                }
                Err(e) => {
                    tracing::error!("Connection error: {e}");
                }
            }

            let jitter = apply_jitter(backoff_secs);
            tracing::info!("Reconnecting in {jitter:.1}s...");
            tokio::time::sleep(Duration::from_secs_f64(jitter)).await;

            backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
        }
    }

    async fn connect_and_report(&self) -> anyhow::Result<()> {
        let ws_url = build_ws_url(&self.config)?;
        tracing::info!("Connecting to {ws_url}...");

        let (ws_stream, _response) = connect_async(&ws_url).await?;
        tracing::info!("WebSocket connected");

        let (mut write, mut read) = ws_stream.split();

        // Wait for Welcome message
        let report_interval = match read.next().await {
            Some(Ok(Message::Text(text))) => {
                let msg: ServerMessage = serde_json::from_str(&text)?;
                match msg {
                    ServerMessage::Welcome {
                        server_id,
                        report_interval,
                        ..
                    } => {
                        tracing::info!("Welcome from server {server_id}, interval={report_interval}s");
                        report_interval
                    }
                    other => {
                        tracing::warn!("Expected Welcome, got: {other:?}");
                        DEFAULT_REPORT_INTERVAL
                    }
                }
            }
            Some(Ok(other)) => {
                tracing::warn!("Expected text Welcome message, got: {other:?}");
                DEFAULT_REPORT_INTERVAL
            }
            Some(Err(e)) => return Err(e.into()),
            None => anyhow::bail!("Connection closed before Welcome"),
        };

        // Send SystemInfo
        let mut collector = Collector::new(self.config.collector.enable_temperature);
        let info = collector.system_info();
        let info_msg = AgentMessage::SystemInfo {
            msg_id: uuid::Uuid::new_v4().to_string(),
            info,
        };
        let json = serde_json::to_string(&info_msg)?;
        write.send(Message::Text(json.into())).await?;
        tracing::info!("Sent SystemInfo");

        // Main loop: send reports and handle server messages
        let mut report_interval = interval(Duration::from_secs(report_interval as u64));
        report_interval.tick().await; // consume first immediate tick

        loop {
            tokio::select! {
                _ = report_interval.tick() => {
                    let report = collector.collect();
                    let msg = AgentMessage::Report(report);
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent report");
                }
                server_msg = read.next() => {
                    match server_msg {
                        Some(Ok(Message::Text(text))) => {
                            self.handle_server_message(&text, &mut write).await?;
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Server closed connection");
                            return Ok(());
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            tracing::error!("WebSocket error: {e}");
                            return Err(e.into());
                        }
                        None => {
                            tracing::info!("WebSocket stream ended");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    async fn handle_server_message<S>(&self, text: &str, write: &mut S) -> anyhow::Result<()>
    where
        S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        let msg: ServerMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to parse server message: {e}");
                return Ok(());
            }
        };

        match msg {
            ServerMessage::Ping => {
                let pong = serde_json::to_string(&AgentMessage::Pong)?;
                write.send(Message::Text(pong.into())).await?;
                tracing::debug!("Responded to Ping with Pong");
            }
            ServerMessage::Exec {
                task_id,
                command,
                timeout,
            } => {
                tracing::info!("Executing command (task_id={task_id}): {command}");
                let result = execute_command(&task_id, &command, timeout).await;
                let msg = AgentMessage::TaskResult {
                    msg_id: uuid::Uuid::new_v4().to_string(),
                    result,
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
                tracing::info!("Sent TaskResult for task_id={task_id}");
            }
            ServerMessage::Ack { msg_id } => {
                tracing::debug!("Received Ack for msg_id={msg_id}");
            }
            ServerMessage::Welcome { .. } => {
                tracing::warn!("Unexpected second Welcome message");
            }
            ServerMessage::PingTasksSync { .. } => {
                tracing::debug!("Received PingTasksSync (not yet implemented)");
            }
            ServerMessage::TerminalClose { session_id } => {
                tracing::debug!("Received TerminalClose for session_id={session_id} (not yet implemented)");
            }
            ServerMessage::Upgrade {
                version,
                download_url,
            } => {
                tracing::info!("Upgrade available: v{version} at {download_url} (not yet implemented)");
            }
        }

        Ok(())
    }
}

async fn execute_command(
    task_id: &str,
    command: &str,
    timeout: Option<u32>,
) -> serverbee_common::types::TaskResult {
    let timeout_secs = timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT_SECS);

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs as u64),
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                combined.push('\n');
                combined.push_str(&stderr);
            }
            if combined.len() > MAX_TASK_OUTPUT_SIZE {
                combined.truncate(MAX_TASK_OUTPUT_SIZE);
                combined.push_str("\n... (output truncated)");
            }
            serverbee_common::types::TaskResult {
                task_id: task_id.to_string(),
                output: combined,
                exit_code: output.status.code().unwrap_or(-1),
            }
        }
        Ok(Err(e)) => serverbee_common::types::TaskResult {
            task_id: task_id.to_string(),
            output: format!("Failed to execute command: {e}"),
            exit_code: -1,
        },
        Err(_) => serverbee_common::types::TaskResult {
            task_id: task_id.to_string(),
            output: format!("Command timed out after {timeout_secs}s"),
            exit_code: -1,
        },
    }
}

fn build_ws_url(config: &AgentConfig) -> anyhow::Result<String> {
    let base = config.server_url.trim_end_matches('/');
    let ws_base = if base.starts_with("https://") {
        base.replacen("https://", "wss://", 1)
    } else if base.starts_with("http://") {
        base.replacen("http://", "ws://", 1)
    } else {
        format!("ws://{base}")
    };
    Ok(format!("{ws_base}/api/agent/ws?token={}", config.token))
}

fn apply_jitter(base_secs: u64) -> f64 {
    let base = base_secs as f64;
    let jitter_range = base * JITTER_FACTOR;
    let mut rng = rand::thread_rng();
    let jitter: f64 = rng.gen_range(-jitter_range..=jitter_range);
    (base + jitter).max(0.5)
}
