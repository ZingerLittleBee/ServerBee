use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serverbee_common::constants::{DEFAULT_COMMAND_TIMEOUT_SECS, MAX_TASK_OUTPUT_SIZE};
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::collector::Collector;
use crate::config::AgentConfig;
use crate::file_manager::{FileEvent, FileManager};
use crate::network_prober::NetworkProber;
use crate::pinger::PingManager;
use crate::terminal::{TerminalEvent, TerminalManager};
use serverbee_common::types::NetworkProbeResultData;

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
        use serverbee_common::constants::*;

        let ws_url = build_ws_url(&self.config)?;
        tracing::info!("Connecting to {ws_url}...");

        let capabilities = Arc::new(AtomicU32::new(u32::MAX));

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
                        capabilities: caps,
                        ..
                    } => {
                        tracing::info!("Welcome from server {server_id}, interval={report_interval}s");
                        if let Some(c) = caps {
                            capabilities.store(c, Ordering::SeqCst);
                        } else {
                            capabilities.store(u32::MAX, Ordering::SeqCst);
                        }
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
        let mut collector = Collector::new(
            self.config.collector.enable_temperature,
            self.config.collector.enable_gpu,
        );
        let info = collector.system_info();
        let info_msg = AgentMessage::SystemInfo {
            msg_id: uuid::Uuid::new_v4().to_string(),
            info: serverbee_common::types::SystemInfo {
                protocol_version: PROTOCOL_VERSION,
                ..info
            },
        };
        let json = serde_json::to_string(&info_msg)?;
        write.send(Message::Text(json.into())).await?;
        tracing::info!("Sent SystemInfo");

        // Ping probe manager
        let (ping_tx, mut ping_rx) = mpsc::channel(256);
        let mut ping_manager = PingManager::new(ping_tx, Arc::clone(&capabilities));

        // Terminal session manager
        let (term_tx, mut term_rx) = mpsc::channel(256);
        let mut terminal_manager = TerminalManager::new(term_tx, Arc::clone(&capabilities));

        // Network probe manager
        let (network_probe_tx, mut network_probe_rx) = mpsc::channel::<NetworkProbeResultData>(256);
        let mut network_prober = NetworkProber::new(network_probe_tx, Arc::clone(&capabilities));

        // File manager
        let (file_tx, mut file_rx) = mpsc::channel::<FileEvent>(16);
        let file_manager = FileManager::new(
            self.config.file.clone(),
            Arc::clone(&capabilities),
        );

        // Channel for background command execution results
        let (cmd_result_tx, mut cmd_result_rx) = mpsc::channel::<AgentMessage>(32);

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
                Some(ping_result) = ping_rx.recv() => {
                    let msg = AgentMessage::PingResult(ping_result);
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent PingResult");
                }
                Some(cmd_msg) = cmd_result_rx.recv() => {
                    let json = serde_json::to_string(&cmd_msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent background command result");
                }
                Some(term_event) = term_rx.recv() => {
                    let msg = match term_event {
                        TerminalEvent::Output { session_id, data } => {
                            AgentMessage::TerminalOutput { session_id, data }
                        }
                        TerminalEvent::Started { session_id } => {
                            AgentMessage::TerminalStarted { session_id }
                        }
                        TerminalEvent::Error { session_id, error } => {
                            AgentMessage::TerminalError { session_id, error }
                        }
                        TerminalEvent::Exited { session_id } => {
                            terminal_manager.close(&session_id);
                            AgentMessage::TerminalError {
                                session_id,
                                error: "Session exited".to_string(),
                            }
                        }
                    };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                }
                Some(first_result) = network_probe_rx.recv() => {
                    let mut results = vec![first_result];
                    // Drain any additional results that arrived at the same time
                    while let Ok(additional) = network_probe_rx.try_recv() {
                        results.push(additional);
                    }
                    let count = results.len();
                    let msg = AgentMessage::NetworkProbeResults { results };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent NetworkProbeResults ({count} results)");
                }
                Some(file_event) = file_rx.recv() => {
                    let msg: AgentMessage = file_event.into();
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent file event");
                }
                server_msg = read.next() => {
                    match server_msg {
                        Some(Ok(Message::Text(text))) => {
                            self.handle_server_message(&text, &mut write, &mut ping_manager, &mut terminal_manager, &mut network_prober, &cmd_result_tx, &capabilities, &file_manager, &file_tx).await?;
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Server closed connection");
                            ping_manager.stop_all();
                            terminal_manager.close_all();
                            network_prober.stop_all();
                            file_manager.cancel_all_transfers();
                            return Ok(());
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            tracing::error!("WebSocket error: {e}");
                            ping_manager.stop_all();
                            terminal_manager.close_all();
                            network_prober.stop_all();
                            file_manager.cancel_all_transfers();
                            return Err(e.into());
                        }
                        None => {
                            tracing::info!("WebSocket stream ended");
                            ping_manager.stop_all();
                            terminal_manager.close_all();
                            network_prober.stop_all();
                            file_manager.cancel_all_transfers();
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_server_message<S>(
        &self,
        text: &str,
        write: &mut S,
        ping_manager: &mut PingManager,
        terminal_manager: &mut TerminalManager,
        network_prober: &mut NetworkProber,
        cmd_result_tx: &mpsc::Sender<AgentMessage>,
        capabilities: &Arc<AtomicU32>,
        file_manager: &FileManager,
        file_tx: &mpsc::Sender<FileEvent>,
    ) -> anyhow::Result<()>
    where
        S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        use serverbee_common::constants::*;

        let msg: ServerMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to parse server message: {e}");
                return Ok(());
            }
        };

        match msg {
            ServerMessage::CapabilitiesSync { capabilities: caps } => {
                tracing::info!("Capabilities updated: {caps}");
                capabilities.store(caps, Ordering::SeqCst);
                network_prober.resync_capabilities();
            }
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
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_EXEC) {
                    tracing::warn!("Exec denied: capability disabled (task_id={task_id})");
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: Some(task_id),
                        session_id: None,
                        capability: "exec".to_string(),
                    };
                    let tx = cmd_result_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(denied).await;
                    });
                    return Ok(());
                }
                tracing::info!("Executing command (task_id={task_id}): {command}");
                let tx = cmd_result_tx.clone();
                tokio::spawn(async move {
                    let result = execute_command(&task_id, &command, timeout).await;
                    let msg = AgentMessage::TaskResult {
                        msg_id: uuid::Uuid::new_v4().to_string(),
                        result,
                    };
                    if tx.send(msg).await.is_err() {
                        tracing::warn!("Failed to send TaskResult for task_id={task_id}: channel closed");
                    } else {
                        tracing::info!("TaskResult ready for task_id={task_id}");
                    }
                });
            }
            ServerMessage::Ack { msg_id } => {
                tracing::debug!("Received Ack for msg_id={msg_id}");
            }
            ServerMessage::Welcome { .. } => {
                tracing::warn!("Unexpected second Welcome message");
            }
            ServerMessage::PingTasksSync { tasks } => {
                tracing::info!("Received PingTasksSync with {} tasks", tasks.len());
                ping_manager.sync(tasks);
            }
            ServerMessage::TerminalOpen {
                session_id,
                rows,
                cols,
            } => {
                tracing::info!("Opening terminal session {session_id} ({cols}x{rows})");
                terminal_manager.open(session_id, rows, cols);
            }
            ServerMessage::TerminalInput { session_id, data } => {
                terminal_manager.write_input(&session_id, &data);
            }
            ServerMessage::TerminalResize {
                session_id,
                rows,
                cols,
            } => {
                tracing::debug!("Resizing terminal {session_id} to {cols}x{rows}");
                terminal_manager.resize(&session_id, rows, cols);
            }
            ServerMessage::TerminalClose { session_id } => {
                tracing::debug!("Closing terminal session {session_id}");
                terminal_manager.close(&session_id);
            }
            ServerMessage::Upgrade {
                version,
                download_url,
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_UPGRADE) {
                    tracing::warn!("Upgrade denied: capability disabled");
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: None,
                        session_id: None,
                        capability: "upgrade".to_string(),
                    };
                    let json = serde_json::to_string(&denied)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                tracing::info!("Upgrade requested: v{version} from {download_url}");
                tokio::spawn(async move {
                    if let Err(e) = perform_upgrade(&version, &download_url).await {
                        tracing::error!("Upgrade to v{version} failed: {e}");
                    }
                });
            }
            ServerMessage::NetworkProbeSync { targets, interval, packet_count } => {
                tracing::info!(
                    "Received NetworkProbeSync: {} targets, interval={}s, packet_count={}",
                    targets.len(),
                    interval,
                    packet_count
                );
                network_prober.sync(targets, interval, packet_count);
            }
            // --- File management messages ---
            ServerMessage::FileList { msg_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileListResult { msg_id, path, entries: vec![], error: Some("File capability disabled".into()) };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.list_dir(&path).await;
                let msg = match result {
                    Ok(entries) => AgentMessage::FileListResult { msg_id, path, entries, error: None },
                    Err(e) => AgentMessage::FileListResult { msg_id, path, entries: vec![], error: Some(e.to_string()) },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileStat { msg_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileStatResult { msg_id, entry: None, error: Some("File capability disabled".into()) };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.stat(&path).await;
                let msg = match result {
                    Ok(entry) => AgentMessage::FileStatResult { msg_id, entry: Some(entry), error: None },
                    Err(e) => AgentMessage::FileStatResult { msg_id, entry: None, error: Some(e.to_string()) },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileRead { msg_id, path, max_size } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileReadResult { msg_id, content: None, error: Some("File capability disabled".into()) };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.read_file(&path, max_size).await;
                let msg = match result {
                    Ok(content) => AgentMessage::FileReadResult { msg_id, content: Some(content), error: None },
                    Err(e) => AgentMessage::FileReadResult { msg_id, content: None, error: Some(e.to_string()) },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileWrite { msg_id, path, content } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult { msg_id, success: false, error: Some("File capability disabled".into()) };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.write_file(&path, &content).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult { msg_id, success: true, error: None },
                    Err(e) => AgentMessage::FileOpResult { msg_id, success: false, error: Some(e.to_string()) },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileDelete { msg_id, path, recursive } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult { msg_id, success: false, error: Some("File capability disabled".into()) };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.delete(&path, recursive).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult { msg_id, success: true, error: None },
                    Err(e) => AgentMessage::FileOpResult { msg_id, success: false, error: Some(e.to_string()) },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileMkdir { msg_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult { msg_id, success: false, error: Some("File capability disabled".into()) };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.mkdir(&path).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult { msg_id, success: true, error: None },
                    Err(e) => AgentMessage::FileOpResult { msg_id, success: false, error: Some(e.to_string()) },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileMove { msg_id, from, to } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult { msg_id, success: false, error: Some("File capability disabled".into()) };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.rename_path(&from, &to).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult { msg_id, success: true, error: None },
                    Err(e) => AgentMessage::FileOpResult { msg_id, success: false, error: Some(e.to_string()) },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileDownloadStart { transfer_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileDownloadError { transfer_id, error: "File capability disabled".into() };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                file_manager.start_download(transfer_id, path, file_tx.clone());
            }
            ServerMessage::FileDownloadCancel { transfer_id } => {
                file_manager.cancel_download(&transfer_id);
            }
            ServerMessage::FileUploadStart { transfer_id, path, size } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileUploadError { transfer_id, error: "File capability disabled".into() };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                match file_manager.start_upload(transfer_id.clone(), path, size).await {
                    Ok(()) => {
                        let msg = AgentMessage::FileUploadAck { transfer_id, offset: 0 };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                    Err(e) => {
                        let msg = AgentMessage::FileUploadError { transfer_id, error: e.to_string() };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                }
            }
            ServerMessage::FileUploadChunk { transfer_id, offset, data } => {
                match file_manager.receive_chunk(&transfer_id, offset, &data).await {
                    Ok(new_offset) => {
                        let msg = AgentMessage::FileUploadAck { transfer_id, offset: new_offset };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                    Err(e) => {
                        let msg = AgentMessage::FileUploadError { transfer_id, error: e.to_string() };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                }
            }
            ServerMessage::FileUploadEnd { transfer_id } => {
                match file_manager.finish_upload(&transfer_id).await {
                    Ok(()) => {
                        let msg = AgentMessage::FileUploadComplete { transfer_id };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                    Err(e) => {
                        let msg = AgentMessage::FileUploadError { transfer_id, error: e.to_string() };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                }
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

/// Download a new agent binary, verify checksum, replace current binary, and restart.
async fn perform_upgrade(version: &str, download_url: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use sha2::{Digest, Sha256};

    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("bak");

    tracing::info!("Downloading agent v{version}...");
    let client = reqwest::Client::new();
    let response = client
        .get(download_url)
        .header("User-Agent", "ServerBee-Agent")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Download failed with status {}",
            response.status()
        );
    }

    // Extract expected checksum from header if present
    let expected_checksum = response
        .headers()
        .get("x-checksum-sha256")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let bytes = response.bytes().await?;
    tracing::info!("Downloaded {} bytes", bytes.len());

    // Verify checksum if provided
    if let Some(expected) = &expected_checksum {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual = format!("{:x}", hasher.finalize());
        if actual != *expected {
            anyhow::bail!(
                "Checksum mismatch: expected {expected}, got {actual}"
            );
        }
        tracing::info!("Checksum verified");
    }

    // Write to temporary file
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Backup current binary and replace
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }
    std::fs::rename(&current_exe, &backup_path)?;
    std::fs::rename(&tmp_path, &current_exe)?;

    tracing::info!("Agent binary replaced. Restarting...");

    // Restart: exec the new binary with the same args
    let args: Vec<String> = std::env::args().collect();
    let mut cmd = std::process::Command::new(&current_exe);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    cmd.spawn()?;

    // Exit current process
    std::process::exit(0);
}
