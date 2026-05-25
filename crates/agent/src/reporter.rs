use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serverbee_common::constants::{
    CapabilityDeniedReason, DEFAULT_COMMAND_TIMEOUT_SECS, MAX_TASK_OUTPUT_SIZE, has_capability,
};
use serverbee_common::protocol::{AgentMessage, ServerMessage, UpgradeStage};
use serverbee_common::types::{NetworkInterface, NetworkProbeResultData};
use sysinfo::Networks;
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::collector::Collector;
use crate::config::AgentConfig;
use crate::docker::DockerManager;
use crate::file_manager::{FileEvent, FileManager};
use crate::firewall::FirewallManager;
use crate::firewall::nft::CliNftExecutor;
use crate::ip_quality::{RunResult, UnlockChecker};
use crate::network_prober::NetworkProber;
use crate::pinger::PingManager;
use crate::register;
use crate::terminal::{TerminalEvent, TerminalManager};

const MAX_BACKOFF_SECS: u64 = 30;
const JITTER_FACTOR: f64 = 0.2;
const MAX_REREGISTER_ATTEMPTS: u32 = 3;
const DOCKER_RETRY_SECS: u64 = 30;

static UPGRADE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

enum ServerMessageOutcome {
    Continue,
    Reconnect,
}

pub struct Reporter {
    config: AgentConfig,
    fingerprint: String,
    agent_local_capabilities: u32,
    firewall_manager: Arc<FirewallManager>,
}

impl Reporter {
    pub fn new(config: AgentConfig, fingerprint: String, agent_local_capabilities: u32) -> Self {
        let firewall_manager = Arc::new(FirewallManager::new(Arc::new(CliNftExecutor)));
        Self {
            config,
            fingerprint,
            agent_local_capabilities,
            firewall_manager,
        }
    }

    /// Convenience wrapper around [`Self::run_with_external`] for tests
    /// and historical callers that don't need a security stream.
    #[cfg(test)]
    #[allow(dead_code)]
    pub async fn run(&mut self) {
        self.run_with_external(None).await
    }

    /// Run with an optional external agent-message stream attached
    /// (currently sourced from [`crate::security::SecurityManager`]).
    pub async fn run_with_external(
        &mut self,
        mut external_rx: Option<mpsc::Receiver<AgentMessage>>,
    ) {
        let mut backoff_secs: u64 = 1;
        let mut reregister_attempts: u32 = 0;

        loop {
            match self.connect_and_report(&mut external_rx).await {
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
                                 giving up re-registration. Check server URL and ensure the WebSocket \
                                 endpoint is accessible (token is sent via query parameter)."
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

    async fn connect_and_report(
        &mut self,
        external_rx: &mut Option<mpsc::Receiver<AgentMessage>>,
    ) -> anyhow::Result<()> {
        use serverbee_common::constants::*;

        tracing::info!("Connecting to {}...", build_ws_url(&self.config)?);

        let capabilities = Arc::new(AtomicU32::new(self.agent_local_capabilities));
        let server_capabilities = Arc::new(AtomicU32::new(u32::MAX));

        let request = build_ws_request(&self.config)?;
        let (ws_stream, _response) = connect_async(request).await?;
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
                        let server_caps = server_capabilities_from_welcome(caps);
                        let effective_caps = compute_effective_capabilities(
                            server_caps,
                            self.agent_local_capabilities,
                        );
                        tracing::info!(
                            "Welcome from server {server_id}, interval={report_interval}s"
                        );
                        server_capabilities.store(server_caps, Ordering::SeqCst);
                        capabilities.store(effective_caps, Ordering::SeqCst);
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
        // Synchronous interface-only scan — never blocks the startup path.
        // The background `spawn_external_ip_refresh` (after cmd_result_tx
        // is wired up below) will emit a follow-up IpChanged once the
        // public IP is discovered.
        let (initial_ipv4, initial_ipv6) = derive_interface_ips(&initial_ips);
        // Feed the agent's primary IP into the firewall guardrail before
        // any blocklist push can arrive. Background discovery may refine
        // this shortly.
        if let Some(ip) = primary_external_ip(initial_ipv4.as_deref(), initial_ipv6.as_deref()) {
            self.firewall_manager.set_external_ip(Some(ip)).await;
        }
        let info_msg = AgentMessage::SystemInfo {
            msg_id: uuid::Uuid::new_v4().to_string(),
            info: serverbee_common::types::SystemInfo {
                protocol_version: PROTOCOL_VERSION,
                features,
                ipv4: initial_ipv4.clone(),
                ipv6: initial_ipv6.clone(),
                ..info
            },
            agent_local_capabilities: Some(self.agent_local_capabilities),
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

        // IP quality unlock checker
        let (unlock_result_tx, mut unlock_result_rx) = mpsc::channel::<RunResult>(8);
        let unlock_checker = UnlockChecker::new(Arc::clone(&capabilities), unlock_result_tx);

        // Seed the checker with the interface-derived public IP so the very first
        // run has a non-empty egress_ip even on stable VPS deployments where the
        // external IP never "changes" (i.e. IpChanged is never emitted because
        // spawn_external_ip_refresh confirms the same IP as the baseline).
        // Mirror the ipv4.or(ipv6) preference used in the IpChanged interception
        // arm of the select! loop below.
        {
            use serverbee_common::constants::CAP_IP_QUALITY;
            if has_capability(capabilities.load(Ordering::SeqCst), CAP_IP_QUALITY) {
                let seed_ip = initial_ipv4.clone().or_else(|| initial_ipv6.clone());
                // Only seed when we have a non-empty interface-derived IP.
                // If neither is available the checker stays at None and will be
                // populated by IpChanged from spawn_external_ip_refresh.
                if seed_ip.is_some() {
                    unlock_checker.notify_ip_changed(seed_ip);
                }
            }
        }

        // File manager
        let (file_tx, mut file_rx) = mpsc::channel::<FileEvent>(16);
        let file_manager = FileManager::new(self.config.file.clone(), Arc::clone(&capabilities));

        // Channel for background command execution results.
        let (cmd_result_tx, mut cmd_result_rx) = mpsc::channel::<AgentMessage>(32);

        // Fire-and-forget: refine the just-sent SystemInfo IPs with an
        // externally-observed public IP. If discovery yields something
        // different, an IpChanged is emitted via cmd_result_tx and forwarded
        // by the main select! loop below. Startup is never blocked on
        // potentially-slow public IP services.
        spawn_external_ip_refresh(
            self.config.ip_change.external_ip_urls.clone(),
            initial_ips.clone(),
            initial_ipv4,
            initial_ipv6,
            cmd_result_tx.clone(),
            Arc::clone(&self.firewall_manager),
        );

        // External agent-message source (e.g. SecurityManager). Optional —
        // non-Linux builds and unit tests skip this. The borrow is held
        // for the lifetime of this connection only; the receiver itself
        // lives across reconnects.

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
        let ip_external_urls = self.config.ip_change.external_ip_urls.clone();

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
                    // Intercept IpChanged to notify the UnlockChecker before forwarding.
                    if let AgentMessage::IpChanged { ref ipv4, ref ipv6, .. } = cmd_msg {
                        use serverbee_common::constants::CAP_IP_QUALITY;
                        if has_capability(capabilities.load(Ordering::SeqCst), CAP_IP_QUALITY) {
                            // Prefer IPv4 egress; fall back to IPv6.
                            let new_ip = ipv4.clone().or_else(|| ipv6.clone());
                            unlock_checker.notify_ip_changed(new_ip);
                        }
                    }
                    let json = serde_json::to_string(&cmd_msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent background command result");
                }
                Some(run_result) = unlock_result_rx.recv() => {
                    let msg = AgentMessage::UnlockResults {
                        egress_ip: run_result.egress_ip,
                        results: run_result.results,
                        checked_at: run_result.checked_at,
                    };
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent UnlockResults");
                }
                Some(external_msg) = async {
                    match external_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending::<Option<AgentMessage>>().await,
                    }
                } => {
                    let json = serde_json::to_string(&external_msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent external agent message");
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
                // IP change detection — spawned off the WS hot path so a
                // slow external-IP service can never block report/ping/
                // terminal traffic.
                _ = ip_check_interval.tick(), if ip_change_enabled => {
                    let new_ips = collect_interface_ips();
                    if new_ips != cached_ips {
                        tracing::info!("IP change detected (interface delta)");
                        let (old_v4, old_v6) = derive_interface_ips(&cached_ips);
                        cached_ips = new_ips.clone();
                        // Pass the previously-reported IPs as baseline so
                        // the spawned task emits exactly one IpChanged
                        // covering both the interface delta and any
                        // external override discovered.
                        spawn_external_ip_refresh(
                            ip_external_urls.clone(),
                            new_ips,
                            old_v4,
                            old_v6,
                            cmd_result_tx.clone(),
                            Arc::clone(&self.firewall_manager),
                        );
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
                            match self.handle_server_message(&text, &mut write, &mut ping_manager, &mut terminal_manager, &mut network_prober, &cmd_result_tx, &capabilities, &server_capabilities, &file_manager, &file_tx, &mut docker_manager, &mut docker_available, &mut docker_stats_interval, &unlock_checker).await? {
                                ServerMessageOutcome::Continue => {}
                                ServerMessageOutcome::Reconnect => {
                                    ping_manager.stop_all();
                                    terminal_manager.close_all();
                                    network_prober.stop_all();
                                    unlock_checker.stop();
                                    file_manager.cancel_all_transfers();
                                    if let Some(dm) = docker_manager.as_mut() {
                                        dm.cleanup();
                                    }
                                    return Ok(());
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Server closed connection");
                            ping_manager.stop_all();
                            terminal_manager.close_all();
                            network_prober.stop_all();
                            unlock_checker.stop();
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
                            unlock_checker.stop();
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
                            unlock_checker.stop();
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
        &mut self,
        text: &str,
        write: &mut S,
        ping_manager: &mut PingManager,
        terminal_manager: &mut TerminalManager,
        network_prober: &mut NetworkProber,
        cmd_result_tx: &mpsc::Sender<AgentMessage>,
        capabilities: &Arc<AtomicU32>,
        server_capabilities: &Arc<AtomicU32>,
        file_manager: &FileManager,
        file_tx: &mpsc::Sender<FileEvent>,
        docker_manager: &mut Option<DockerManager>,
        docker_available: &mut bool,
        docker_stats_interval: &mut Option<tokio::time::Interval>,
        unlock_checker: &UnlockChecker,
    ) -> anyhow::Result<ServerMessageOutcome>
    where
        S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        use serverbee_common::constants::*;

        let msg: ServerMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to parse server message: {e}");
                return Ok(ServerMessageOutcome::Continue);
            }
        };

        match msg {
            ServerMessage::CapabilitiesSync { capabilities: caps } => {
                let old_caps = sync_capability_state(
                    capabilities,
                    server_capabilities,
                    caps,
                    self.agent_local_capabilities,
                );
                let effective_caps = capabilities.load(Ordering::SeqCst);
                tracing::info!("Capabilities updated: server={caps}, effective={effective_caps}");
                network_prober.resync_capabilities();

                // If Docker capability was removed, clean up
                if has_capability(old_caps, CAP_DOCKER)
                    && !has_capability(effective_caps, CAP_DOCKER)
                {
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
                    let denied_reason = capability_denied_reason(
                        server_capabilities.load(Ordering::SeqCst),
                        self.agent_local_capabilities,
                        CAP_EXEC,
                    );
                    tracing::warn!("Exec denied: capability disabled (task_id={task_id})");
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: Some(task_id),
                        session_id: None,
                        capability: "exec".to_string(),
                        reason: denied_reason,
                    };
                    let tx = cmd_result_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(denied).await;
                    });
                    return Ok(ServerMessageOutcome::Continue);
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
                job_id,
                ..
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_UPGRADE) {
                    let denied_reason = capability_denied_reason(
                        server_capabilities.load(Ordering::SeqCst),
                        self.agent_local_capabilities,
                        CAP_UPGRADE,
                    );
                    tracing::warn!("Upgrade denied: capability disabled");
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: None,
                        session_id: None,
                        capability: "upgrade".to_string(),
                        reason: denied_reason,
                    };
                    let json = serde_json::to_string(&denied)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(ServerMessageOutcome::Continue);
                }

                if UPGRADE_IN_PROGRESS
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    let tx = cmd_result_tx.clone();
                    tokio::spawn(async move {
                        emit_upgrade_failure(
                            &tx,
                            job_id,
                            version,
                            UpgradeStage::Downloading,
                            "another upgrade is already running".to_string(),
                            None,
                        )
                        .await;
                    });
                    return Ok(ServerMessageOutcome::Continue);
                }

                tracing::info!("Upgrade requested: v{version} (pinned source)");
                let upgrade_cfg = self.config.upgrade.clone();
                let tx = cmd_result_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        perform_upgrade(&version, &upgrade_cfg, job_id, tx.clone()).await
                    {
                        tracing::error!("Upgrade to v{version} failed: {e}");
                        UPGRADE_IN_PROGRESS.store(false, Ordering::SeqCst);
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
                protocol,
            } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_PING_ICMP) {
                    let denied_reason = capability_denied_reason(
                        server_capabilities.load(Ordering::SeqCst),
                        self.agent_local_capabilities,
                        CAP_PING_ICMP,
                    );
                    tracing::warn!(
                        "Traceroute denied: capability disabled (request_id={request_id})"
                    );
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: Some(request_id),
                        session_id: None,
                        capability: "ping_icmp".to_string(),
                        reason: denied_reason,
                    };
                    let tx = cmd_result_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(denied).await;
                    });
                    return Ok(ServerMessageOutcome::Continue);
                }

                // Input validation: target must be domain or IP only.
                if !crate::traceroute::is_valid_traceroute_target(&target) {
                    tracing::warn!(
                        "Traceroute rejected: invalid target '{target}' (request_id={request_id})"
                    );
                    let tx = cmd_result_tx.clone();
                    let request_id_c = request_id.clone();
                    let target_c = target.clone();
                    tokio::spawn(async move {
                        let _ = tx
                            .send(AgentMessage::TracerouteRoundUpdate {
                                request_id: request_id_c,
                                target: target_c,
                                round: 0,
                                total_rounds: 0,
                                hops: vec![],
                                completed: true,
                                error: Some(
                                    "Invalid target: must be a domain or IP address".into(),
                                ),
                            })
                            .await;
                    });
                    return Ok(ServerMessageOutcome::Continue);
                }

                let proto =
                    protocol.unwrap_or(serverbee_common::protocol::TraceProtocol::Icmp);
                tracing::info!(
                    "Executing traceroute to {target} (max_hops={max_hops}, request_id={request_id}, protocol={proto:?})"
                );
                crate::traceroute::spawn_traceroute(
                    request_id,
                    target,
                    max_hops,
                    proto,
                    cmd_result_tx.clone(),
                );
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
                    return Ok(ServerMessageOutcome::Continue);
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
            // Firewall blocklist variants — dispatched to the FirewallManager
            // state machine; any returned ack is sent straight back over the
            // WebSocket.
            msg @ (ServerMessage::BlocklistSync { .. }
            | ServerMessage::BlocklistAdd { .. }
            | ServerMessage::BlocklistRemove { .. }
            | ServerMessage::BlocklistReset) => {
                if let Some(reply) = self.firewall_manager.handle(msg).await {
                    let json = serde_json::to_string(&reply)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent firewall blocklist ack");
                }
            }
            ServerMessage::IpQualitySync { services, interval_hours } => {
                let caps = capabilities.load(Ordering::SeqCst);
                if has_capability(caps, CAP_IP_QUALITY) {
                    tracing::info!(
                        "Received IpQualitySync: {} services, interval={}h",
                        services.len(),
                        interval_hours
                    );
                    unlock_checker.sync(services, interval_hours).await;
                } else {
                    tracing::debug!(
                        "IpQualitySync received but CAP_IP_QUALITY not effective — ignoring"
                    );
                }
            }
            ServerMessage::IpQualityRunNow => {
                let caps = capabilities.load(Ordering::SeqCst);
                if has_capability(caps, CAP_IP_QUALITY) {
                    tracing::info!("Received IpQualityRunNow");
                    unlock_checker.run_now();
                } else {
                    tracing::debug!(
                        "IpQualityRunNow received but CAP_IP_QUALITY not effective — ignoring"
                    );
                }
            }
        }

        Ok(ServerMessageOutcome::Continue)
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

fn build_ws_request(
    config: &AgentConfig,
) -> anyhow::Result<tokio_tungstenite::tungstenite::http::Request<()>> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

    let ws_url = format!("{}?token={}", build_ws_url(config)?, config.token);
    let mut request = ws_url.into_client_request()?;
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {}", config.token).parse()?);
    Ok(request)
}

fn server_capabilities_from_welcome(server_caps: Option<u32>) -> u32 {
    server_caps.unwrap_or(u32::MAX)
}

fn compute_effective_capabilities(server_caps: u32, agent_local_capabilities: u32) -> u32 {
    serverbee_common::constants::effective_capabilities(server_caps, agent_local_capabilities)
}

fn sync_capability_state(
    capabilities: &Arc<AtomicU32>,
    server_capabilities: &Arc<AtomicU32>,
    server_caps: u32,
    agent_local_capabilities: u32,
) -> u32 {
    let old_caps = capabilities.load(Ordering::SeqCst);
    let effective_caps = compute_effective_capabilities(server_caps, agent_local_capabilities);
    server_capabilities.store(server_caps, Ordering::SeqCst);
    capabilities.store(effective_caps, Ordering::SeqCst);
    old_caps
}

fn capability_denied_reason(
    server_caps: u32,
    agent_local_capabilities: u32,
    cap_bit: u32,
) -> CapabilityDeniedReason {
    if !has_capability(server_caps, cap_bit) {
        CapabilityDeniedReason::ServerCapabilityDisabled
    } else {
        debug_assert!(!has_capability(agent_local_capabilities, cap_bit));
        CapabilityDeniedReason::AgentCapabilityDisabled
    }
}

fn should_refresh_registration(config: &AgentConfig, error: &anyhow::Error) -> bool {
    !config.enrollment_code.is_empty()
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
/// Convert the primary IPv4/IPv6 strings emitted by `derive_primary_ips` into
/// a single parsed `IpAddr`, preferring IPv4 (matches the server-side guardrail
/// convention). Returns `None` when both inputs are missing or unparseable.
fn primary_external_ip(ipv4: Option<&str>, ipv6: Option<&str>) -> Option<IpAddr> {
    if let Some(s) = ipv4
        && let Ok(ip) = s.parse::<IpAddr>()
    {
        return Some(ip);
    }
    if let Some(s) = ipv6
        && let Ok(ip) = s.parse::<IpAddr>()
    {
        return Some(ip);
    }
    None
}

fn is_private_ipv4_str(s: &str) -> bool {
    s.parse::<std::net::Ipv4Addr>()
        .map(|v4| v4.is_private() || v4.is_link_local() || v4.is_loopback())
        .unwrap_or(false)
}

fn is_private_ipv6_str(s: &str) -> bool {
    s.parse::<std::net::Ipv6Addr>()
        .map(|v6| {
            let seg = v6.segments();
            // Unique local fc00::/7, link-local fe80::/10, loopback ::1
            (seg[0] & 0xfe00) == 0xfc00 || (seg[0] & 0xffc0) == 0xfe80 || v6.is_loopback()
        })
        .unwrap_or(false)
}

/// Pure interface-list scan. Synchronous and fast — never touches the network.
/// Prefers the first public (routable) address; remembers the first private
/// one as a fallback so containerised agents on a docker bridge still report
/// something for the UI even before external discovery completes.
fn derive_interface_ips(interfaces: &[NetworkInterface]) -> (Option<String>, Option<String>) {
    let mut public_ipv4: Option<String> = None;
    let mut private_ipv4: Option<String> = None;
    let mut public_ipv6: Option<String> = None;
    let mut private_ipv6: Option<String> = None;

    for iface in interfaces {
        for ip in &iface.ipv4 {
            if is_private_ipv4_str(ip) {
                if private_ipv4.is_none() {
                    private_ipv4 = Some(ip.clone());
                }
            } else if public_ipv4.is_none() {
                public_ipv4 = Some(ip.clone());
            }
        }
        for ip in &iface.ipv6 {
            if is_private_ipv6_str(ip) {
                if private_ipv6.is_none() {
                    private_ipv6 = Some(ip.clone());
                }
            } else if public_ipv6.is_none() {
                public_ipv6 = Some(ip.clone());
            }
        }
        if public_ipv4.is_some() && public_ipv6.is_some() {
            break;
        }
    }

    (public_ipv4.or(private_ipv4), public_ipv6.or(private_ipv6))
}

/// Overlay an externally-observed IP on top of interface-derived primaries.
/// Replaces whichever stack the external IP belongs to.
fn apply_external_ip(
    ipv4: Option<String>,
    ipv6: Option<String>,
    external: Option<String>,
) -> (Option<String>, Option<String>) {
    match external {
        Some(ext) if ext.contains(':') => (ipv4, Some(ext)),
        Some(ext) => (Some(ext), ipv6),
        None => (ipv4, ipv6),
    }
}

/// Discover the agent's public IP in the background and emit an
/// `IpChanged` message via `cmd_result_tx` when the merged result differs
/// from `baseline_*`. Fire-and-forget; the WS hot path is never blocked.
fn spawn_external_ip_refresh(
    urls: Vec<String>,
    interfaces: Vec<NetworkInterface>,
    baseline_ipv4: Option<String>,
    baseline_ipv6: Option<String>,
    cmd_result_tx: mpsc::Sender<AgentMessage>,
    firewall_manager: Arc<FirewallManager>,
) {
    tokio::spawn(async move {
        let (iface_v4, iface_v6) = derive_interface_ips(&interfaces);
        let external = if urls.is_empty() {
            None
        } else {
            match fetch_external_ip(&urls).await {
                Ok(ip) => Some(ip),
                Err(e) => {
                    tracing::debug!("External IP discovery failed: {e}");
                    None
                }
            }
        };
        let (new_v4, new_v6) = apply_external_ip(iface_v4, iface_v6, external);
        if new_v4 == baseline_ipv4 && new_v6 == baseline_ipv6 {
            return;
        }
        tracing::info!(
            "IP refresh: ipv4 {:?} -> {:?}, ipv6 {:?} -> {:?}",
            baseline_ipv4,
            new_v4,
            baseline_ipv6,
            new_v6
        );
        if let Some(ip) = primary_external_ip(new_v4.as_deref(), new_v6.as_deref()) {
            firewall_manager.set_external_ip(Some(ip)).await;
        }
        let msg = AgentMessage::IpChanged {
            ipv4: new_v4,
            ipv6: new_v6,
            interfaces,
        };
        if cmd_result_tx.send(msg).await.is_err() {
            tracing::debug!("IpChanged emission dropped: channel closed");
        }
    });
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::config::{
        CollectorConfig, FileConfig, IpChangeConfig, LogConfig, SecurityConfig, UpgradeConfig,
    };
    use serverbee_common::constants::{
        CAP_DEFAULT, CAP_EXEC, CAP_FILE, CAP_PING_ICMP, CapabilityDeniedReason,
    };
    use tokio_tungstenite::tungstenite::http::Response;

    #[test]
    fn test_effective_capabilities_from_welcome_masks_server_and_local_caps() {
        assert_eq!(
            compute_effective_capabilities(CAP_EXEC | CAP_FILE, CAP_FILE),
            CAP_FILE
        );
    }

    #[test]
    fn test_effective_capabilities_from_welcome_defaults_to_local_caps_when_missing() {
        assert_eq!(
            compute_effective_capabilities(server_capabilities_from_welcome(None), CAP_DEFAULT),
            CAP_DEFAULT
        );
    }

    #[test]
    fn test_capabilities_sync_recomputes_effective_caps_instead_of_overwriting_local_policy() {
        let capabilities = Arc::new(AtomicU32::new(CAP_FILE));
        let server_capabilities = Arc::new(AtomicU32::new(u32::MAX));
        let old_caps =
            sync_capability_state(&capabilities, &server_capabilities, CAP_EXEC, CAP_FILE);

        assert_eq!(old_caps, CAP_FILE);
        assert_eq!(capabilities.load(Ordering::SeqCst), 0);
        assert_eq!(server_capabilities.load(Ordering::SeqCst), CAP_EXEC);
    }

    #[test]
    fn test_capability_denied_reason_uses_server_policy_when_server_disables_capability() {
        assert_eq!(
            capability_denied_reason(0, CAP_EXEC, CAP_EXEC),
            CapabilityDeniedReason::ServerCapabilityDisabled
        );
    }

    #[test]
    fn test_capability_denied_reason_uses_agent_policy_when_server_allows_capability() {
        assert_eq!(
            capability_denied_reason(CAP_PING_ICMP, 0, CAP_PING_ICMP),
            CapabilityDeniedReason::AgentCapabilityDisabled
        );
    }

    #[test]
    fn test_should_refresh_registration_on_unauthorized_handshake() {
        let config = AgentConfig {
            server_url: "http://127.0.0.1:9527".to_string(),
            token: "stale-token".to_string(),
            enrollment_code: "dev-key".to_string(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
            upgrade: UpgradeConfig::default(),
            security: SecurityConfig::default(),
        };
        let err = anyhow::Error::new(tokio_tungstenite::tungstenite::Error::Http(
            Response::builder().status(401).body(None).unwrap(),
        ));

        assert!(should_refresh_registration(&config, &err));
    }

    #[test]
    fn test_should_not_refresh_registration_without_enrollment_code() {
        let config = AgentConfig {
            server_url: "http://127.0.0.1:9527".to_string(),
            token: "stale-token".to_string(),
            enrollment_code: String::new(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
            upgrade: UpgradeConfig::default(),
            security: SecurityConfig::default(),
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
            enrollment_code: "dev-key".to_string(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
            upgrade: UpgradeConfig::default(),
            security: SecurityConfig::default(),
        };
        let err = anyhow::Error::new(tokio_tungstenite::tungstenite::Error::Http(
            Response::builder().status(500).body(None).unwrap(),
        ));

        assert!(!should_refresh_registration(&config, &err));
    }

    #[test]
    fn test_build_ws_request_carries_query_token_and_authorization_header() {
        let config = AgentConfig {
            server_url: "https://example.com".to_string(),
            token: "agent-token-123".to_string(),
            enrollment_code: String::new(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
            upgrade: UpgradeConfig::default(),
            security: SecurityConfig::default(),
        };

        let request = build_ws_request(&config).expect("request should build");

        assert_eq!(
            request.uri().to_string(),
            "wss://example.com/api/agent/ws?token=agent-token-123"
        );
        assert_eq!(
            request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer agent-token-123")
        );
    }

    #[test]
    fn test_is_private_ipv4_str_classifies_docker_bridge_and_rfc1918() {
        assert!(is_private_ipv4_str("172.17.0.1"));
        assert!(is_private_ipv4_str("172.18.0.1"));
        assert!(is_private_ipv4_str("10.0.0.5"));
        assert!(is_private_ipv4_str("192.168.1.1"));
        assert!(is_private_ipv4_str("169.254.1.1"));
        assert!(is_private_ipv4_str("127.0.0.1"));
        assert!(!is_private_ipv4_str("8.8.8.8"));
        assert!(!is_private_ipv4_str("203.0.113.1"));
    }

    #[test]
    fn test_is_private_ipv6_str_classifies_ula_link_local_and_loopback() {
        assert!(is_private_ipv6_str("::1"));
        assert!(is_private_ipv6_str("fe80::1"));
        assert!(is_private_ipv6_str("fc00::1"));
        assert!(is_private_ipv6_str("fd12:3456:789a::1"));
        assert!(!is_private_ipv6_str("2001:db8::1"));
        assert!(!is_private_ipv6_str("2606:4700:4700::1111"));
    }

    #[test]
    fn test_derive_interface_ips_prefers_public_over_docker_bridge() {
        // Simulate an agent inside a docker container: docker0 has the bridge gateway,
        // eth0 has the real public IP. We must pick the public one, not 172.17.0.1.
        let interfaces = vec![
            NetworkInterface {
                name: "docker0".to_string(),
                ipv4: vec!["172.17.0.1".to_string()],
                ipv6: vec![],
            },
            NetworkInterface {
                name: "eth0".to_string(),
                ipv4: vec!["203.0.113.42".to_string()],
                ipv6: vec!["2001:db8::1".to_string()],
            },
        ];
        let (v4, v6) = derive_interface_ips(&interfaces);
        assert_eq!(v4.as_deref(), Some("203.0.113.42"));
        assert_eq!(v6.as_deref(), Some("2001:db8::1"));
    }

    #[test]
    fn test_derive_interface_ips_falls_back_to_private_when_no_public_available() {
        // If only private/docker addresses exist, still report one so the UI has
        // something to display. The server-side GeoIP path will detect and skip
        // it for country resolution.
        let interfaces = vec![NetworkInterface {
            name: "docker0".to_string(),
            ipv4: vec!["172.17.0.1".to_string()],
            ipv6: vec!["fe80::1".to_string()],
        }];
        let (v4, v6) = derive_interface_ips(&interfaces);
        assert_eq!(v4.as_deref(), Some("172.17.0.1"));
        assert_eq!(v6.as_deref(), Some("fe80::1"));
    }

    #[test]
    fn test_apply_external_ip_overrides_matching_stack() {
        // External IPv4 should replace ipv4 only.
        let (v4, v6) = apply_external_ip(
            Some("172.17.0.1".to_string()),
            Some("2001:db8::1".to_string()),
            Some("203.0.113.42".to_string()),
        );
        assert_eq!(v4.as_deref(), Some("203.0.113.42"));
        assert_eq!(v6.as_deref(), Some("2001:db8::1"));

        // External IPv6 should replace ipv6 only.
        let (v4, v6) = apply_external_ip(
            Some("10.0.0.1".to_string()),
            None,
            Some("2606:4700::1".to_string()),
        );
        assert_eq!(v4.as_deref(), Some("10.0.0.1"));
        assert_eq!(v6.as_deref(), Some("2606:4700::1"));

        // No external IP → pass-through.
        let (v4, v6) = apply_external_ip(
            Some("10.0.0.1".to_string()),
            Some("fe80::1".to_string()),
            None,
        );
        assert_eq!(v4.as_deref(), Some("10.0.0.1"));
        assert_eq!(v6.as_deref(), Some("fe80::1"));
    }

    #[tokio::test]
    async fn test_fetch_external_ip_rejects_invalid_payload() {
        // Reachable endpoint that returns HTML rather than an IP — must be rejected
        // rather than being trusted as a primary IP.
        let result = fetch_external_ip_once("https://example.com").await;
        assert!(result.is_err(), "non-IP response must be rejected");
    }

    #[tokio::test]
    async fn test_fetch_external_ip_returns_err_on_empty_url_list() {
        let urls: Vec<String> = vec![];
        let result = fetch_external_ip(&urls).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_external_ip_falls_through_to_next_url_on_failure() {
        // First URL is unroutable / fails fast; the next one must be attempted.
        // Use a non-existent local port to fail quickly without hitting the
        // network.
        let urls = vec![
            "http://127.0.0.1:1/never".to_string(),
            "http://127.0.0.1:2/never".to_string(),
        ];
        let result = fetch_external_ip(&urls).await;
        // Both fail — but the function should have *tried* both and surfaced
        // the last error, not panicked or hung.
        assert!(result.is_err());
    }
}

/// Fetch external IP address from a single remote service.
/// Limits response to 256 bytes via streaming to prevent memory exhaustion
/// even when the server omits Content-Length.
async fn fetch_external_ip_once(url: &str) -> anyhow::Result<String> {
    const MAX_IP_RESPONSE: usize = 256;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
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
    // Validate before returning so a misbehaving endpoint can't poison the
    // primary IP with HTML / garbage.
    if ip.parse::<std::net::IpAddr>().is_err() {
        anyhow::bail!("External IP response is not a valid IP: {ip:?}");
    }
    Ok(ip)
}

/// Try each URL in order; return the first successful, validated response.
async fn fetch_external_ip(urls: &[String]) -> anyhow::Result<String> {
    if urls.is_empty() {
        anyhow::bail!("No external IP URLs configured");
    }
    let mut last_err: Option<anyhow::Error> = None;
    for url in urls {
        match fetch_external_ip_once(url).await {
            Ok(ip) => return Ok(ip),
            Err(e) => {
                tracing::debug!("External IP query to {url} failed: {e}");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("All external IP services failed")))
}

async fn emit_upgrade_progress(
    tx: &mpsc::Sender<AgentMessage>,
    job_id: Option<String>,
    version: &str,
    stage: UpgradeStage,
) {
    let message = AgentMessage::UpgradeProgress {
        msg_id: uuid::Uuid::new_v4().to_string(),
        job_id,
        target_version: version.to_string(),
        stage,
    };

    if tx.send(message).await.is_err() {
        tracing::warn!("Failed to emit upgrade progress: channel closed");
    }
}

async fn emit_upgrade_failure(
    tx: &mpsc::Sender<AgentMessage>,
    job_id: Option<String>,
    version: String,
    stage: UpgradeStage,
    error: String,
    backup_path: Option<String>,
) {
    let message = AgentMessage::UpgradeResult {
        msg_id: uuid::Uuid::new_v4().to_string(),
        job_id,
        target_version: version,
        stage,
        error,
        backup_path,
    };

    if tx.send(message).await.is_err() {
        tracing::warn!("Failed to emit upgrade failure: channel closed");
    }
}

/// Pinned-source 升级:Server 仅提供 version;来源由本地 upgrade 配置决定。
async fn perform_upgrade(
    version: &str,
    upgrade_cfg: &crate::config::UpgradeConfig,
    job_id: Option<String>,
    tx: mpsc::Sender<AgentMessage>,
) -> anyhow::Result<()> {
    use crate::upgrade::{
        build_upgrade_client, checksum_for, current_asset_name, derive_urls, ensure_upgrade,
        normalize_spki_pin,
    };
    use sha2::{Digest, Sha256};
    use std::io::Write;

    macro_rules! fail {
        ($stage:expr, $msg:expr) => {{
            let msg: String = $msg;
            emit_upgrade_failure(&tx, job_id.clone(), version.to_string(), $stage, msg.clone(), None)
                .await;
            anyhow::bail!(msg);
        }};
    }

    emit_upgrade_progress(&tx, job_id.clone(), version, UpgradeStage::Downloading).await;

    // 1. 防降级
    let current = serverbee_common::constants::VERSION;
    if let Err(e) = ensure_upgrade(current, version) {
        fail!(UpgradeStage::Downloading, format!("anti-downgrade: {e}"));
    }

    // 2. SPKI pin 规范化(启动时已 fail-fast 校验过格式,这里再防御性规范化一次)
    let spki = match normalize_spki_pin(&upgrade_cfg.release_cert_spki_sha256) {
        Ok(v) => v,
        Err(e) => fail!(UpgradeStage::Downloading, format!("invalid SPKI pin: {e}")),
    };

    // 3. 推导 URL(忽略 Server 的 download_url/sha256)
    let (binary_url, checksums_url) =
        match derive_urls(&upgrade_cfg.release_repo_url, version) {
            Ok(v) => v,
            Err(e) => fail!(UpgradeStage::Downloading, format!("derive url: {e}")),
        };

    // 4. 专用 client
    let client = match build_upgrade_client(spki.as_deref()) {
        Ok(c) => c,
        Err(e) => fail!(UpgradeStage::Downloading, format!("tls client: {e}")),
    };

    tracing::info!("Downloading agent v{version} from pinned source {binary_url}");

    // 5. 拉 checksums.txt
    let checksums = match client.get(&checksums_url).send().await {
        Ok(r) if r.status().is_success() => match r.text().await {
            Ok(t) => t,
            Err(e) => fail!(UpgradeStage::Downloading, format!("read checksums: {e}")),
        },
        Ok(r) => fail!(
            UpgradeStage::Downloading,
            format!("checksums HTTP {}", r.status())
        ),
        Err(e) => fail!(UpgradeStage::Downloading, format!("fetch checksums: {e}")),
    };
    let asset = current_asset_name();
    let want_hash = match checksum_for(&checksums, asset) {
        Ok(h) => h,
        Err(e) => fail!(UpgradeStage::Verifying, format!("parse checksums: {e}")),
    };

    // 6. 下载二进制
    let bytes = match client.get(&binary_url).send().await {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) => b,
            Err(e) => fail!(UpgradeStage::Downloading, format!("read binary: {e}")),
        },
        Ok(r) => fail!(
            UpgradeStage::Downloading,
            format!("binary HTTP {}", r.status())
        ),
        Err(e) => fail!(UpgradeStage::Downloading, format!("fetch binary: {e}")),
    };

    emit_upgrade_progress(&tx, job_id.clone(), version, UpgradeStage::Verifying).await;

    // 7. 校验哈希(对照已从 pinned 源取得的 checksums)
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != want_hash {
        fail!(
            UpgradeStage::Verifying,
            format!("checksum mismatch: expected {want_hash}, got {actual}")
        );
    }
    tracing::info!("Checksum verified against pinned checksums.txt");

    // 8. 落盘 + 替换 + 重启(沿用原逻辑)
    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("bak");

    emit_upgrade_progress(&tx, job_id.clone(), version, UpgradeStage::Installing).await;

    // 文件交换:任一 I/O 失败都要走 fail!(Installing),否则 Server 永远收不到失败、job 卡死。
    let swap = || -> anyhow::Result<()> {
        {
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;
        }
        if backup_path.exists() {
            std::fs::remove_file(&backup_path)?;
        }
        std::fs::rename(&current_exe, &backup_path)?;
        std::fs::rename(&tmp_path, &current_exe)?;
        Ok(())
    };
    if let Err(e) = swap() {
        fail!(UpgradeStage::Installing, format!("install: {e}"));
    }

    tracing::info!("Agent binary replaced. Restarting...");
    emit_upgrade_progress(&tx, job_id.clone(), version, UpgradeStage::Restarting).await;
    let args: Vec<String> = std::env::args().collect();
    let mut cmd = std::process::Command::new(&current_exe);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    if let Err(e) = cmd.spawn() {
        emit_upgrade_failure(
            &tx,
            job_id,
            version.to_string(),
            UpgradeStage::Restarting,
            e.to_string(),
            Some(backup_path.display().to_string()),
        )
        .await;
        return Err(e.into());
    }
    std::process::exit(0);
}
