use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serverbee_common::constants::{DEFAULT_COMMAND_TIMEOUT_SECS, MAX_TASK_OUTPUT_SIZE};
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use serverbee_common::types::{NetworkInterface, NetworkProbeResultData, TracerouteHop};
use sysinfo::Networks;
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::collector::Collector;
use crate::config::AgentConfig;
use crate::docker::DockerManager;
use crate::file_manager::{FileEvent, FileManager};
use crate::network_prober::NetworkProber;
use crate::pinger::PingManager;
use crate::register;
use crate::terminal::{TerminalEvent, TerminalManager};

const MAX_BACKOFF_SECS: u64 = 30;
const JITTER_FACTOR: f64 = 0.2;
const MAX_REREGISTER_ATTEMPTS: u32 = 3;
const DOCKER_RETRY_SECS: u64 = 30;

pub struct Reporter {
    config: AgentConfig,
    fingerprint: String,
}

impl Reporter {
    pub fn new(config: AgentConfig, fingerprint: String) -> Self {
        Self {
            config,
            fingerprint,
        }
    }

    pub async fn run(&mut self) {
        let mut backoff_secs: u64 = 1;
        let mut reregister_attempts: u32 = 0;

        loop {
            match self.connect_and_report().await {
                Ok(()) => {
                    tracing::info!("Connection closed normally, reconnecting...");
                    backoff_secs = 1;
                    reregister_attempts = 0;
                }
                Err(e) => {
                    if should_refresh_registration(&self.config, &e) {
                        if reregister_attempts >= MAX_REREGISTER_ATTEMPTS {
                            tracing::error!(
                                "Agent token rejected {reregister_attempts} times consecutively, \
                                 giving up re-registration. Check server URL and reverse proxy \
                                 configuration (Authorization header must be forwarded for WebSocket)."
                            );
                        } else {
                            reregister_attempts += 1;
                            tracing::warn!(
                                "Stored agent token was rejected, attempting re-registration \
                                 ({reregister_attempts}/{MAX_REREGISTER_ATTEMPTS})"
                            );

                            match register::register_agent(&self.config, &self.fingerprint).await {
                                Ok((server_id, token)) => {
                                    tracing::info!(
                                        "Re-registration successful for server {server_id}"
                                    );
                                    if let Err(save_err) = register::save_token(&token) {
                                        tracing::warn!(
                                            "Failed to save refreshed token: {save_err}"
                                        );
                                    }
                                    self.config.token = token;
                                    // Do NOT skip backoff — prevents tight re-registration loop
                                }
                                Err(register_err) => {
                                    tracing::error!("Re-registration failed: {register_err}");
                                }
                            }
                        }
                    } else {
                        reregister_attempts = 0;
                    }

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

        let ws_url = format!("{}?token={}", build_ws_url(&self.config)?, self.config.token);
        tracing::info!("Connecting to {}...", build_ws_url(&self.config)?);

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
                        tracing::info!(
                            "Welcome from server {server_id}, interval={report_interval}s"
                        );
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

        // Docker manager setup
        let (docker_tx, mut docker_rx) = mpsc::channel::<AgentMessage>(256);
        let mut docker_manager: Option<DockerManager> = None;
        let mut docker_available = false;

        // Try to initialize Docker connection
        match DockerManager::try_new(docker_tx.clone(), Arc::clone(&capabilities)) {
            Ok(dm) => match dm.verify_connection().await {
                Ok(()) => {
                    tracing::info!("Docker daemon connected");
                    docker_available = true;
                    docker_manager = Some(dm);
                }
                Err(e) => {
                    tracing::info!("Docker daemon not available: {e}");
                }
            },
            Err(e) => {
                tracing::info!("Docker not available: {e}");
            }
        }

        // Docker retry interval (only used when docker_manager is None)
        let mut docker_retry_interval = interval(Duration::from_secs(DOCKER_RETRY_SECS));
        docker_retry_interval.tick().await; // consume immediate tick

        // Separate stats interval (managed by start/stop stats commands)
        // Uses a long default that gets replaced when stats are requested.
        let mut docker_stats_interval: Option<tokio::time::Interval> = None;

        // Build features list
        let mut features = Vec::new();
        if docker_available {
            features.push("docker".to_string());
        }

        // Send SystemInfo
        let mut collector = Collector::new(
            self.config.collector.enable_temperature,
            self.config.collector.enable_gpu,
        );
        let info = collector.system_info();
        let initial_ips = collect_interface_ips();
        let (initial_ipv4, initial_ipv6) = derive_primary_ips(
            &initial_ips,
            self.config.ip_change.check_external_ip,
            &self.config.ip_change.external_ip_url,
        )
        .await;
        let info_msg = AgentMessage::SystemInfo {
            msg_id: uuid::Uuid::new_v4().to_string(),
            info: serverbee_common::types::SystemInfo {
                protocol_version: PROTOCOL_VERSION,
                features,
                ipv4: initial_ipv4,
                ipv6: initial_ipv6,
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
        let file_manager = FileManager::new(self.config.file.clone(), Arc::clone(&capabilities));

        // Channel for background command execution results
        let (cmd_result_tx, mut cmd_result_rx) = mpsc::channel::<AgentMessage>(32);

        // IP change detection setup
        let ip_change_enabled = self.config.ip_change.enabled;
        let mut ip_check_interval =
            interval(Duration::from_secs(self.config.ip_change.interval_secs));
        ip_check_interval.tick().await; // consume immediate tick
        let mut cached_ips = if ip_change_enabled {
            collect_interface_ips()
        } else {
            Vec::new()
        };
        let ip_check_external = self.config.ip_change.check_external_ip;
        let ip_external_url = self.config.ip_change.external_ip_url.clone();

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
                // Docker messages from DockerManager background tasks
                Some(docker_msg) = docker_rx.recv() => {
                    let json = serde_json::to_string(&docker_msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent Docker message");
                }
                // Docker stats polling (uses separate interval to avoid borrow conflicts)
                Some(_) = async {
                    match docker_stats_interval.as_mut() {
                        Some(iv) => Some(iv.tick().await),
                        None => None,
                    }
                } => {
                    if let Some(dm) = docker_manager.as_mut()
                        && let Err(e) = dm.poll_stats().await
                    {
                        tracing::warn!("Docker stats polling failed: {e}");
                        self.demote_docker_runtime(
                            &mut write,
                            &mut docker_manager,
                            &mut docker_available,
                            &mut docker_stats_interval,
                        )
                        .await?;
                    }
                }
                // IP change detection
                _ = ip_check_interval.tick(), if ip_change_enabled => {
                    let new_ips = collect_interface_ips();
                    if new_ips != cached_ips {
                        tracing::info!("IP change detected");
                        let (primary_ipv4, primary_ipv6) =
                            derive_primary_ips(&new_ips, ip_check_external, &ip_external_url).await;
                        let msg = AgentMessage::IpChanged {
                            ipv4: primary_ipv4,
                            ipv6: primary_ipv6,
                            interfaces: new_ips.clone(),
                        };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                        tracing::debug!("Sent IpChanged");
                        cached_ips = new_ips;
                    }
                }
                // Docker retry (reconnect when docker is unavailable)
                _ = docker_retry_interval.tick(), if docker_manager.is_none() => {
                    tracing::debug!("Retrying Docker connection...");
                    match DockerManager::try_new(docker_tx.clone(), Arc::clone(&capabilities)) {
                        Ok(dm) => {
                            match dm.verify_connection().await {
                                Ok(()) => {
                                    tracing::info!("Docker daemon now available");
                                    docker_manager = Some(dm);
                                    docker_available = true;
                                    // Notify server about features change
                                    let msg = AgentMessage::FeaturesUpdate {
                                        features: vec!["docker".to_string()],
                                    };
                                    let json = serde_json::to_string(&msg)?;
                                    write.send(Message::Text(json.into())).await?;
                                }
                                Err(e) => {
                                    tracing::debug!("Docker still not available: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("Docker still not available: {e}");
                        }
                    }
                }
                server_msg = read.next() => {
                    match server_msg {
                        Some(Ok(Message::Text(text))) => {
                            self.handle_server_message(&text, &mut write, &mut ping_manager, &mut terminal_manager, &mut network_prober, &cmd_result_tx, &capabilities, &file_manager, &file_tx, &mut docker_manager, &mut docker_available, &mut docker_stats_interval).await?;
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Server closed connection");
                            ping_manager.stop_all();
                            terminal_manager.close_all();
                            network_prober.stop_all();
                            file_manager.cancel_all_transfers();
                            if let Some(dm) = docker_manager.as_mut() {
                                dm.cleanup();
                            }
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
                            if let Some(dm) = docker_manager.as_mut() {
                                dm.cleanup();
                            }
                            return Err(e.into());
                        }
                        None => {
                            tracing::info!("WebSocket stream ended");
                            ping_manager.stop_all();
                            terminal_manager.close_all();
                            network_prober.stop_all();
                            file_manager.cancel_all_transfers();
                            if let Some(dm) = docker_manager.as_mut() {
                                dm.cleanup();
                            }
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
        docker_manager: &mut Option<DockerManager>,
        docker_available: &mut bool,
        docker_stats_interval: &mut Option<tokio::time::Interval>,
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
                let old_caps = capabilities.load(Ordering::SeqCst);
                capabilities.store(caps, Ordering::SeqCst);
                network_prober.resync_capabilities();

                // If Docker capability was removed, clean up
                if has_capability(old_caps, CAP_DOCKER) && !has_capability(caps, CAP_DOCKER) {
                    tracing::info!("Docker capability revoked, cleaning up");
                    if let Some(dm) = docker_manager.as_mut() {
                        dm.cleanup();
                    }
                    *docker_stats_interval = None;
                }
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
                        tracing::warn!(
                            "Failed to send TaskResult for task_id={task_id}: channel closed"
                        );
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
                sha256,
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
                    if let Err(e) = perform_upgrade(&version, &download_url, &sha256).await {
                        tracing::error!("Upgrade to v{version} failed: {e}");
                    }
                });
            }
            ServerMessage::NetworkProbeSync {
                targets,
                interval,
                packet_count,
            } => {
                tracing::info!(
                    "Received NetworkProbeSync: {} targets, interval={}s, packet_count={}",
                    targets.len(),
                    interval,
                    packet_count
                );
                network_prober.sync(targets, interval, packet_count);
            }
            ServerMessage::Traceroute {
                request_id,
                target,
                max_hops,
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_PING_ICMP) {
                    tracing::warn!(
                        "Traceroute denied: capability disabled (request_id={request_id})"
                    );
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: Some(request_id),
                        session_id: None,
                        capability: "ping_icmp".to_string(),
                    };
                    let tx = cmd_result_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(denied).await;
                    });
                    return Ok(());
                }

                // Input validation: target must be domain or IP only
                if !is_valid_traceroute_target(&target) {
                    tracing::warn!(
                        "Traceroute rejected: invalid target '{target}' (request_id={request_id})"
                    );
                    let tx = cmd_result_tx.clone();
                    tokio::spawn(async move {
                        let msg = AgentMessage::TracerouteResult {
                            request_id,
                            target,
                            hops: vec![],
                            completed: true,
                            error: Some("Invalid target: must be a domain or IP address".into()),
                        };
                        let _ = tx.send(msg).await;
                    });
                    return Ok(());
                }

                tracing::info!(
                    "Executing traceroute to {target} (max_hops={max_hops}, request_id={request_id})"
                );
                let tx = cmd_result_tx.clone();
                tokio::spawn(async move {
                    let msg = execute_traceroute(&request_id, &target, max_hops).await;
                    if tx.send(msg).await.is_err() {
                        tracing::warn!(
                            "Failed to send TracerouteResult for request_id={request_id}: channel closed"
                        );
                    }
                });
            }
            // --- File management messages ---
            ServerMessage::FileList { msg_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileListResult {
                        msg_id,
                        path,
                        entries: vec![],
                        error: Some("File capability disabled".into()),
                    };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.list_dir(&path).await;
                let msg = match result {
                    Ok(entries) => AgentMessage::FileListResult {
                        msg_id,
                        path,
                        entries,
                        error: None,
                    },
                    Err(e) => AgentMessage::FileListResult {
                        msg_id,
                        path,
                        entries: vec![],
                        error: Some(e.to_string()),
                    },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileStat { msg_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileStatResult {
                        msg_id,
                        entry: None,
                        error: Some("File capability disabled".into()),
                    };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.stat(&path).await;
                let msg = match result {
                    Ok(entry) => AgentMessage::FileStatResult {
                        msg_id,
                        entry: Some(entry),
                        error: None,
                    },
                    Err(e) => AgentMessage::FileStatResult {
                        msg_id,
                        entry: None,
                        error: Some(e.to_string()),
                    },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileRead {
                msg_id,
                path,
                max_size,
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileReadResult {
                        msg_id,
                        content: None,
                        error: Some("File capability disabled".into()),
                    };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.read_file(&path, max_size).await;
                let msg = match result {
                    Ok(content) => AgentMessage::FileReadResult {
                        msg_id,
                        content: Some(content),
                        error: None,
                    },
                    Err(e) => AgentMessage::FileReadResult {
                        msg_id,
                        content: None,
                        error: Some(e.to_string()),
                    },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileWrite {
                msg_id,
                path,
                content,
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some("File capability disabled".into()),
                    };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.write_file(&path, &content).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult {
                        msg_id,
                        success: true,
                        error: None,
                    },
                    Err(e) => AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some(e.to_string()),
                    },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileDelete {
                msg_id,
                path,
                recursive,
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some("File capability disabled".into()),
                    };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.delete(&path, recursive).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult {
                        msg_id,
                        success: true,
                        error: None,
                    },
                    Err(e) => AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some(e.to_string()),
                    },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileMkdir { msg_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some("File capability disabled".into()),
                    };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.mkdir(&path).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult {
                        msg_id,
                        success: true,
                        error: None,
                    },
                    Err(e) => AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some(e.to_string()),
                    },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileMove { msg_id, from, to } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let result = AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some("File capability disabled".into()),
                    };
                    let json = serde_json::to_string(&result)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                let result = file_manager.rename_path(&from, &to).await;
                let msg = match result {
                    Ok(()) => AgentMessage::FileOpResult {
                        msg_id,
                        success: true,
                        error: None,
                    },
                    Err(e) => AgentMessage::FileOpResult {
                        msg_id,
                        success: false,
                        error: Some(e.to_string()),
                    },
                };
                let json = serde_json::to_string(&msg)?;
                write.send(Message::Text(json.into())).await?;
            }
            ServerMessage::FileDownloadStart { transfer_id, path } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileDownloadError {
                        transfer_id,
                        error: "File capability disabled".into(),
                    };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                file_manager.start_download(transfer_id, path, file_tx.clone());
            }
            ServerMessage::FileDownloadCancel { transfer_id } => {
                file_manager.cancel_download(&transfer_id);
            }
            ServerMessage::FileUploadStart {
                transfer_id,
                path,
                size,
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_FILE) || !file_manager.is_enabled() {
                    let msg = AgentMessage::FileUploadError {
                        transfer_id,
                        error: "File capability disabled".into(),
                    };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
                match file_manager
                    .start_upload(transfer_id.clone(), path, size)
                    .await
                {
                    Ok(()) => {
                        let msg = AgentMessage::FileUploadAck {
                            transfer_id,
                            offset: 0,
                        };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                    Err(e) => {
                        let msg = AgentMessage::FileUploadError {
                            transfer_id,
                            error: e.to_string(),
                        };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                }
            }
            ServerMessage::FileUploadChunk {
                transfer_id,
                offset,
                data,
            } => {
                match file_manager
                    .receive_chunk(&transfer_id, offset, &data)
                    .await
                {
                    Ok(new_offset) => {
                        let msg = AgentMessage::FileUploadAck {
                            transfer_id,
                            offset: new_offset,
                        };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                    Err(e) => {
                        let msg = AgentMessage::FileUploadError {
                            transfer_id,
                            error: e.to_string(),
                        };
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
                        let msg = AgentMessage::FileUploadError {
                            transfer_id,
                            error: e.to_string(),
                        };
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                }
            }
            // --- Docker messages ---
            ServerMessage::DockerStartStats { interval_secs } => {
                if docker_manager.is_some() {
                    let secs = interval_secs.max(1);
                    tracing::info!("Starting Docker stats polling every {secs}s");
                    let mut iv = tokio::time::interval(Duration::from_secs(secs as u64));
                    iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    *docker_stats_interval = Some(iv);
                } else {
                    tracing::warn!("DockerStartStats received but Docker is not available");
                    let unavailable = AgentMessage::DockerUnavailable { msg_id: None };
                    let json = serde_json::to_string(&unavailable)?;
                    write.send(Message::Text(json.into())).await?;
                }
            }
            ServerMessage::DockerStopStats => {
                tracing::info!("Stopping Docker stats polling");
                *docker_stats_interval = None;
            }
            ServerMessage::DockerListContainers { .. }
            | ServerMessage::DockerLogsStart { .. }
            | ServerMessage::DockerLogsStop { .. }
            | ServerMessage::DockerEventsStart
            | ServerMessage::DockerEventsStop
            | ServerMessage::DockerContainerAction { .. }
            | ServerMessage::DockerGetInfo { .. }
            | ServerMessage::DockerListNetworks { .. }
            | ServerMessage::DockerListVolumes { .. } => {
                if let Some(dm) = docker_manager.as_mut() {
                    if let Err(e) = dm.handle_server_message(msg.clone()).await {
                        tracing::warn!("Docker runtime became unavailable: {e}");
                        self.demote_docker_runtime(
                            write,
                            docker_manager,
                            docker_available,
                            docker_stats_interval,
                        )
                        .await?;
                    }
                } else {
                    tracing::warn!("Docker message received but Docker is not available");
                    let unavailable = AgentMessage::DockerUnavailable {
                        msg_id: docker_request_msg_id(&msg),
                    };
                    let json = serde_json::to_string(&unavailable)?;
                    write.send(Message::Text(json.into())).await?;
                }
            }
        }

        Ok(())
    }
}

impl Reporter {
    async fn demote_docker_runtime<S>(
        &self,
        write: &mut S,
        docker_manager: &mut Option<DockerManager>,
        docker_available: &mut bool,
        docker_stats_interval: &mut Option<tokio::time::Interval>,
    ) -> anyhow::Result<()>
    where
        S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        if let Some(dm) = docker_manager.as_mut() {
            dm.cleanup();
        }
        *docker_manager = None;
        *docker_stats_interval = None;

        if *docker_available {
            *docker_available = false;
            let msg = AgentMessage::FeaturesUpdate { features: vec![] };
            let json = serde_json::to_string(&msg)?;
            write.send(Message::Text(json.into())).await?;
        }

        Ok(())
    }
}

fn docker_request_msg_id(msg: &ServerMessage) -> Option<String> {
    match msg {
        ServerMessage::DockerListContainers { msg_id }
        | ServerMessage::DockerGetInfo { msg_id }
        | ServerMessage::DockerListNetworks { msg_id }
        | ServerMessage::DockerListVolumes { msg_id } => Some(msg_id.clone()),
        ServerMessage::DockerContainerAction { msg_id, .. } => Some(msg_id.clone()),
        _ => None,
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

/// Validate that a traceroute target contains only safe characters (domain or IP).
/// Matches `^[a-zA-Z0-9.\-:]+$` — rejects shell metacharacters.
fn is_valid_traceroute_target(target: &str) -> bool {
    !target.is_empty()
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
}

/// Execute a traceroute command and parse the output into TracerouteHop structures.
async fn execute_traceroute(
    request_id: &str,
    target: &str,
    max_hops: u8,
) -> AgentMessage {
    let timeout_duration = Duration::from_secs(60);

    let result = tokio::time::timeout(timeout_duration, async {
        // Platform-specific command selection
        #[cfg(windows)]
        let cmd_result = {
            tokio::process::Command::new("tracert")
                .args(["-d", "-h", &max_hops.to_string(), target])
                .output()
                .await
        };

        #[cfg(not(windows))]
        let cmd_result = {
            // Try traceroute first
            let traceroute_result = tokio::process::Command::new("traceroute")
                .args(["-n", "-m", &max_hops.to_string(), target])
                .output()
                .await;

            match traceroute_result {
                Ok(output) => Ok(output),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Fall back to mtr
                    tracing::info!("traceroute not found, trying mtr");
                    tokio::process::Command::new("mtr")
                        .args(["-r", "-n", "-c", "3", "-m", &max_hops.to_string(), target])
                        .output()
                        .await
                }
                Err(e) => Err(e),
            }
        };

        cmd_result
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if !output.status.success() {
                // Check for permission errors
                let combined = format!("{stdout}\n{stderr}");
                let error_msg = if combined.to_lowercase().contains("permission")
                    || combined.to_lowercase().contains("operation not permitted")
                {
                    format!("Permission denied: {}", combined.trim())
                } else {
                    format!(
                        "Command exited with code {}: {}",
                        output.status.code().unwrap_or(-1),
                        combined.trim()
                    )
                };
                return AgentMessage::TracerouteResult {
                    request_id: request_id.to_string(),
                    target: target.to_string(),
                    hops: vec![],
                    completed: true,
                    error: Some(error_msg),
                };
            }

            let hops = parse_traceroute_output(&stdout);

            AgentMessage::TracerouteResult {
                request_id: request_id.to_string(),
                target: target.to_string(),
                hops,
                completed: true,
                error: None,
            }
        }
        Ok(Err(e)) => {
            let error_msg = if e.kind() == std::io::ErrorKind::NotFound {
                "traceroute not installed".to_string()
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!("Permission denied: {e}")
            } else {
                format!("Failed to execute traceroute: {e}")
            };
            AgentMessage::TracerouteResult {
                request_id: request_id.to_string(),
                target: target.to_string(),
                hops: vec![],
                completed: true,
                error: Some(error_msg),
            }
        }
        Err(_) => AgentMessage::TracerouteResult {
            request_id: request_id.to_string(),
            target: target.to_string(),
            hops: vec![],
            completed: true,
            error: Some("Traceroute timed out after 60s".to_string()),
        },
    }
}

/// Parse traceroute/mtr output into a list of TracerouteHop.
///
/// Handles standard traceroute output format:
///   1  192.168.1.1  1.234 ms  1.456 ms  1.678 ms
///   2  * * *
///
/// And mtr report format:
///   HOST: hostname                    Loss%   Snt   Last   Avg  Best  Wrst StDev
///   1.|-- 192.168.1.1                 0.0%     3    1.2   1.3   1.1   1.5   0.2
fn parse_traceroute_output(output: &str) -> Vec<TracerouteHop> {
    let mut hops = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try to parse as standard traceroute format
        if let Some(hop) = parse_traceroute_line(trimmed) {
            hops.push(hop);
        } else if let Some(hop) = parse_mtr_line(trimmed) {
            hops.push(hop);
        }
    }

    hops
}

/// Parse a standard traceroute output line.
/// Format: `<hop>  <ip_or_*>  <rtt1> ms  <rtt2> ms  <rtt3> ms`
fn parse_traceroute_line(line: &str) -> Option<TracerouteHop> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    // First token must be a hop number
    let hop: u8 = parts[0].parse().ok()?;

    // If the line is all stars (no response), return a hop with no data
    if parts.len() >= 2 && parts[1] == "*" {
        return Some(TracerouteHop {
            hop,
            ip: None,
            hostname: None,
            rtt1: None,
            rtt2: None,
            rtt3: None,
            asn: None,
        });
    }

    // Second token should be an IP address
    let ip = if parts.len() > 1 && parts[1] != "*" {
        Some(parts[1].to_string())
    } else {
        None
    };

    // Extract RTT values — look for tokens followed by "ms"
    let mut rtts = Vec::new();
    let mut i = 2;
    while i < parts.len() {
        if parts[i] == "*" {
            rtts.push(None);
            i += 1;
        } else if let Ok(rtt) = parts[i].parse::<f64>() {
            // Check if next token is "ms"
            if i + 1 < parts.len() && parts[i + 1] == "ms" {
                rtts.push(Some(rtt));
                i += 2;
            } else {
                rtts.push(Some(rtt));
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    Some(TracerouteHop {
        hop,
        ip,
        hostname: None,
        rtt1: rtts.first().copied().flatten(),
        rtt2: rtts.get(1).copied().flatten(),
        rtt3: rtts.get(2).copied().flatten(),
        asn: None,
    })
}

/// Parse an mtr report line.
/// Format: `<hop>.|-- <ip_or_???>  <loss%>  <snt>  <last>  <avg>  <best>  <wrst> <stdev>`
fn parse_mtr_line(line: &str) -> Option<TracerouteHop> {
    // mtr lines start with a number followed by `.|--`
    if !line.contains(".|--") {
        return None;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }

    // First token: "1.|--"
    let hop_str = parts[0].split(".|").next()?;
    let hop: u8 = hop_str.parse().ok()?;

    // Second token: IP or "???"
    let ip = if parts[1] == "???" {
        None
    } else {
        Some(parts[1].to_string())
    };

    // RTT values: mtr gives Last, Avg, Best, Wrst — use Last for rtt1, Avg for rtt2, Best for rtt3
    // Columns: Loss% Snt Last Avg Best Wrst StDev (indices 2-8)
    let rtt1 = parts.get(4).and_then(|s| s.parse::<f64>().ok()); // Last
    let rtt2 = parts.get(5).and_then(|s| s.parse::<f64>().ok()); // Avg
    let rtt3 = parts.get(6).and_then(|s| s.parse::<f64>().ok()); // Best

    Some(TracerouteHop {
        hop,
        ip,
        hostname: None,
        rtt1,
        rtt2,
        rtt3,
        asn: None,
    })
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
    Ok(format!("{ws_base}/api/agent/ws"))
}

fn should_refresh_registration(
    config: &AgentConfig,
    error: &anyhow::Error,
) -> bool {
    !config.auto_discovery_key.is_empty()
        && matches!(
            error.downcast_ref::<tokio_tungstenite::tungstenite::Error>(),
            Some(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status().as_u16() == 401
        )
}

fn apply_jitter(base_secs: u64) -> f64 {
    let base = base_secs as f64;
    let jitter_range = base * JITTER_FACTOR;
    let mut rng = rand::thread_rng();
    let jitter: f64 = rng.gen_range(-jitter_range..=jitter_range);
    (base + jitter).max(0.5)
}

/// Collect IP addresses from all network interfaces using sysinfo.
fn collect_interface_ips() -> Vec<NetworkInterface> {
    let networks = Networks::new_with_refreshed_list();
    let mut interfaces = Vec::new();

    for (name, data) in networks.iter() {
        let mut ipv4 = Vec::new();
        let mut ipv6 = Vec::new();
        for ip_net in data.ip_networks() {
            match ip_net.addr {
                IpAddr::V4(v4) => ipv4.push(v4.to_string()),
                IpAddr::V6(v6) => ipv6.push(v6.to_string()),
            }
        }
        if !ipv4.is_empty() || !ipv6.is_empty() {
            interfaces.push(NetworkInterface {
                name: name.to_string(),
                ipv4,
                ipv6,
            });
        }
    }

    // Sort for stable comparison
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));
    interfaces
}

/// Derive primary IPv4/IPv6 from the interface list.
/// If `check_external` is true, also query the external IP service.
async fn derive_primary_ips(
    interfaces: &[NetworkInterface],
    check_external: bool,
    external_url: &str,
) -> (Option<String>, Option<String>) {
    let mut primary_ipv4: Option<String> = None;
    let mut primary_ipv6: Option<String> = None;

    // Pick first non-loopback address from interfaces
    for iface in interfaces {
        if primary_ipv4.is_none() {
            for ip in &iface.ipv4 {
                if ip != "127.0.0.1" {
                    primary_ipv4 = Some(ip.clone());
                    break;
                }
            }
        }
        if primary_ipv6.is_none() {
            for ip in &iface.ipv6 {
                if ip != "::1" {
                    primary_ipv6 = Some(ip.clone());
                    break;
                }
            }
        }
        if primary_ipv4.is_some() && primary_ipv6.is_some() {
            break;
        }
    }

    // Optionally query external IP
    if check_external {
        match fetch_external_ip(external_url).await {
            Ok(ext_ip) => {
                // Override primary with externally visible IP
                if ext_ip.contains(':') {
                    primary_ipv6 = Some(ext_ip);
                } else {
                    primary_ipv4 = Some(ext_ip);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to fetch external IP: {e}");
            }
        }
    }

    (primary_ipv4, primary_ipv6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CollectorConfig, FileConfig, IpChangeConfig, LogConfig};
    use tokio_tungstenite::tungstenite::http::Response;

    #[test]
    fn test_should_refresh_registration_on_unauthorized_handshake() {
        let config = AgentConfig {
            server_url: "http://127.0.0.1:9527".to_string(),
            token: "stale-token".to_string(),
            auto_discovery_key: "dev-key".to_string(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
        };
        let err = anyhow::Error::new(tokio_tungstenite::tungstenite::Error::Http(
            Response::builder().status(401).body(None).unwrap(),
        ));

        assert!(should_refresh_registration(&config, &err));
    }

    #[test]
    fn test_should_not_refresh_registration_without_auto_discovery_key() {
        let config = AgentConfig {
            server_url: "http://127.0.0.1:9527".to_string(),
            token: "stale-token".to_string(),
            auto_discovery_key: String::new(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
        };
        let err = anyhow::Error::new(tokio_tungstenite::tungstenite::Error::Http(
            Response::builder().status(401).body(None).unwrap(),
        ));

        assert!(!should_refresh_registration(&config, &err));
    }

    #[test]
    fn test_should_not_refresh_registration_for_non_unauthorized_handshake() {
        let config = AgentConfig {
            server_url: "http://127.0.0.1:9527".to_string(),
            token: "stale-token".to_string(),
            auto_discovery_key: "dev-key".to_string(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
        };
        let err = anyhow::Error::new(tokio_tungstenite::tungstenite::Error::Http(
            Response::builder().status(500).body(None).unwrap(),
        ));

        assert!(!should_refresh_registration(&config, &err));
    }

    #[test]
    fn test_is_valid_traceroute_target() {
        // Valid targets
        assert!(is_valid_traceroute_target("8.8.8.8"));
        assert!(is_valid_traceroute_target("google.com"));
        assert!(is_valid_traceroute_target("sub.example.com"));
        assert!(is_valid_traceroute_target("2001:db8::1"));
        assert!(is_valid_traceroute_target("my-server.example.com"));

        // Invalid targets — shell metacharacters
        assert!(!is_valid_traceroute_target(""));
        assert!(!is_valid_traceroute_target("8.8.8.8; rm -rf /"));
        assert!(!is_valid_traceroute_target("$(whoami)"));
        assert!(!is_valid_traceroute_target("target | cat /etc/passwd"));
        assert!(!is_valid_traceroute_target("host`id`"));
        assert!(!is_valid_traceroute_target("foo&bar"));
        assert!(!is_valid_traceroute_target("target > /tmp/out"));
    }

    #[test]
    fn test_parse_traceroute_standard_output() {
        let output = "\
traceroute to 8.8.8.8 (8.8.8.8), 30 hops max, 60 byte packets
 1  192.168.1.1  1.234 ms  1.456 ms  1.678 ms
 2  10.0.0.1  5.123 ms  5.456 ms  5.789 ms
 3  * * *
 4  8.8.8.8  10.123 ms  10.456 ms  10.789 ms
";
        let hops = parse_traceroute_output(output);
        assert_eq!(hops.len(), 4);

        assert_eq!(hops[0].hop, 1);
        assert_eq!(hops[0].ip, Some("192.168.1.1".to_string()));
        assert_eq!(hops[0].rtt1, Some(1.234));
        assert_eq!(hops[0].rtt2, Some(1.456));
        assert_eq!(hops[0].rtt3, Some(1.678));

        assert_eq!(hops[1].hop, 2);
        assert_eq!(hops[1].ip, Some("10.0.0.1".to_string()));

        assert_eq!(hops[2].hop, 3);
        assert!(hops[2].ip.is_none());
        assert!(hops[2].rtt1.is_none());

        assert_eq!(hops[3].hop, 4);
        assert_eq!(hops[3].ip, Some("8.8.8.8".to_string()));
    }

    #[test]
    fn test_parse_mtr_output() {
        let output = "\
Start: 2026-03-20T10:00:00+0000
HOST: agent                       Loss%   Snt   Last   Avg  Best  Wrst StDev
  1.|-- 192.168.1.1                0.0%     3    1.2   1.3   1.1   1.5   0.2
  2.|-- 10.0.0.1                   0.0%     3    5.0   5.1   4.9   5.3   0.1
  3.|-- ???                       100.0     3    0.0   0.0   0.0   0.0   0.0
";
        let hops = parse_traceroute_output(output);
        assert_eq!(hops.len(), 3);

        assert_eq!(hops[0].hop, 1);
        assert_eq!(hops[0].ip, Some("192.168.1.1".to_string()));
        assert_eq!(hops[0].rtt1, Some(1.2)); // Last
        assert_eq!(hops[0].rtt2, Some(1.3)); // Avg
        assert_eq!(hops[0].rtt3, Some(1.1)); // Best

        assert_eq!(hops[1].hop, 2);
        assert_eq!(hops[1].ip, Some("10.0.0.1".to_string()));

        assert_eq!(hops[2].hop, 3);
        assert!(hops[2].ip.is_none());
    }

    #[test]
    fn test_parse_traceroute_empty_output() {
        let hops = parse_traceroute_output("");
        assert!(hops.is_empty());
    }
}

/// Fetch external IP address from a remote service.
/// Limits response to 256 bytes via streaming to prevent memory exhaustion
/// even when the server omits Content-Length.
async fn fetch_external_ip(url: &str) -> anyhow::Result<String> {
    const MAX_IP_RESPONSE: usize = 256;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let mut resp = client.get(url).send().await?;

    // Early reject if Content-Length declares a large body
    if let Some(len) = resp.content_length()
        && len > MAX_IP_RESPONSE as u64
    {
        anyhow::bail!("External IP response too large: {len} bytes");
    }

    // Stream chunks with a hard cap to prevent OOM from chunked/streaming responses
    let mut buf = Vec::with_capacity(MAX_IP_RESPONSE);
    while let Some(chunk) = resp.chunk().await? {
        if buf.len() + chunk.len() > MAX_IP_RESPONSE {
            anyhow::bail!(
                "External IP response too large: exceeded {} bytes",
                MAX_IP_RESPONSE
            );
        }
        buf.extend_from_slice(&chunk);
    }

    let ip = String::from_utf8_lossy(&buf).trim().to_string();
    Ok(ip)
}

/// Download a new agent binary, verify checksum, replace current binary, and restart.
async fn perform_upgrade(version: &str, download_url: &str, sha256: &str) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    // Validate URL scheme
    if !download_url.starts_with("https://") {
        anyhow::bail!("Upgrade URL must use HTTPS, got: {download_url}");
    }

    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("bak");

    tracing::info!("Downloading agent v{version} from {download_url}...");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10 minute timeout
        .build()?;
    let response = client
        .get(download_url)
        .header("User-Agent", "ServerBee-Agent")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status {}", response.status());
    }

    let bytes = response.bytes().await?;
    tracing::info!("Downloaded {} bytes", bytes.len());

    // Mandatory SHA-256 verification
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = format!("{:x}", hasher.finalize());
    if actual != sha256 {
        anyhow::bail!("Checksum mismatch: expected {sha256}, got {actual}");
    }
    tracing::info!("Checksum verified");

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

    let args: Vec<String> = std::env::args().collect();
    let mut cmd = std::process::Command::new(&current_exe);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    cmd.spawn()?;

    std::process::exit(0);
}
