mod docker_subsystem;
mod file_ops;
mod runtime;
mod wire;

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serverbee_common::protocol::{AgentMessage, ServerMessage, UpgradeStage};
use serverbee_common::types::NetworkInterface;
use sysinfo::Networks;
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use docker_subsystem::DockerTick;
use runtime::ConnectionRuntime;
use wire::send_msg;

use crate::capability_grants::CapabilityAuthority;
use crate::collector::Collector;
use crate::config::AgentConfig;
use crate::firewall::FirewallManager;
use crate::firewall::nft::CliNftExecutor;
use crate::register;
use crate::terminal::TerminalEvent;

const MAX_BACKOFF_SECS: u64 = 30;
const JITTER_FACTOR: f64 = 0.2;
const MAX_REREGISTER_ATTEMPTS: u32 = 3;

pub struct Reporter {
    config: AgentConfig,
    capabilities: Arc<CapabilityAuthority>,
    firewall_manager: Arc<FirewallManager>,
}

impl Reporter {
    pub fn new(config: AgentConfig, capabilities: Arc<CapabilityAuthority>) -> Self {
        let firewall_manager = Arc::new(FirewallManager::new(Arc::new(CliNftExecutor)));
        Self {
            config,
            capabilities,
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

                            match register::register_agent(&mut self.config).await {
                                Ok(register::RegistrationOutcome::Confirmed { server_id }) => {
                                    tracing::info!(
                                        "Re-registration successful for server {server_id}"
                                    );
                                    // Do NOT skip backoff — prevents tight re-registration loop
                                }
                                Ok(register::RegistrationOutcome::Ambiguous) => tracing::warn!(
                                    "Re-registration response was ambiguous; the next WebSocket \
                                     attempt will verify the staged token"
                                ),
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

        // The process-wide authority already folds active temporary grants
        // into the effective caps, so the first SystemInfo reflects grants
        // that survived a restart.
        let capabilities = Arc::clone(&self.capabilities);
        let effective_caps = capabilities.effective();
        let initial_temporary = capabilities.active_grants();

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
                        ..
                    } => {
                        // The server-advertised `capabilities` field is
                        // intentionally ignored: capabilities are agent-owned
                        // and already loaded into `capabilities` above. The
                        // agent enforces purely on its local policy.
                        tracing::info!(
                            "Welcome from server {server_id}, interval={report_interval}s"
                        );
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

        // Connection-scoped command runtime: owns every manager and channel
        // the server-message dispatcher drives. Docker detection runs before
        // SystemInfo so the advertised features list is accurate.
        let (mut runtime, mut events) = ConnectionRuntime::new(
            &self.config,
            Arc::clone(&capabilities),
            Arc::clone(&self.firewall_manager),
        );
        runtime.docker.probe().await;
        let features = runtime.docker.features();

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
            agent_local_capabilities: Some(effective_caps),
            temporary: initial_temporary.clone(),
        };
        send_msg(&mut write, &info_msg).await?;
        tracing::info!("Sent SystemInfo");

        // Seed the checker with the interface-derived public IP so the very first
        // run has a non-empty egress_ip even on stable VPS deployments where the
        // external IP never "changes" (i.e. IpChanged is never emitted because
        // spawn_external_ip_refresh confirms the same IP as the baseline).
        // Mirror the ipv4.or(ipv6) preference used in the IpChanged interception
        // arm of the select! loop below.
        {
            use serverbee_common::constants::CAP_IP_QUALITY;
            if capabilities.has(CAP_IP_QUALITY) {
                let seed_ip = initial_ipv4.clone().or_else(|| initial_ipv6.clone());
                // Only seed when we have a non-empty interface-derived IP.
                // If neither is available the checker stays at None and will be
                // populated by IpChanged from spawn_external_ip_refresh.
                if seed_ip.is_some() {
                    runtime.unlock_checker.notify_ip_changed(seed_ip);
                }
            }
        }

        // Outbound channel for background command results; also feeds the
        // fire-and-forget IP refresh tasks below.
        let cmd_result_tx = runtime.cmd_result_tx.clone();

        // Capability transitions from the process-wide authority: each one is
        // forwarded to the server as CapabilitiesChanged, after reconciling
        // in-flight work owned by this connection.
        let mut transition_rx = capabilities.subscribe_transitions();

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
                    send_msg(&mut write, &msg).await?;
                    tracing::debug!("Sent report");
                }
                Some(ping_result) = events.ping_rx.recv() => {
                    let msg = AgentMessage::PingResult(ping_result);
                    send_msg(&mut write, &msg).await?;
                    tracing::debug!("Sent PingResult");
                }
                Some(cmd_msg) = events.cmd_result_rx.recv() => {
                    // Intercept IpChanged to notify the UnlockChecker before forwarding.
                    if let AgentMessage::IpChanged { ref ipv4, ref ipv6, .. } = cmd_msg {
                        use serverbee_common::constants::CAP_IP_QUALITY;
                        if capabilities.has(CAP_IP_QUALITY) {
                            // Prefer IPv4 egress; fall back to IPv6.
                            let new_ip = ipv4.clone().or_else(|| ipv6.clone());
                            runtime.unlock_checker.notify_ip_changed(new_ip);
                        }
                    }
                    send_msg(&mut write, &cmd_msg).await?;
                    tracing::debug!("Sent background command result");
                }
                transition = transition_rx.recv() => {
                    match transition {
                        Ok(t) => {
                            // Reconcile in-flight work before announcing: a
                            // revoked or expired terminal capability must tear
                            // down live PTY sessions, not just block new ones.
                            if !has_capability(t.effective, CAP_TERMINAL) {
                                for session_id in runtime.terminal_manager.close_all() {
                                    let msg = AgentMessage::TerminalError {
                                        session_id,
                                        error: "Terminal capability revoked or expired".to_string(),
                                    };
                                    send_msg(&mut write, &msg).await?;
                                }
                            }
                            let msg = AgentMessage::CapabilitiesChanged {
                                msg_id: uuid::Uuid::new_v4().to_string(),
                                capabilities: t.effective,
                                temporary: t.temporary,
                                changes: t.changes,
                            };
                            send_msg(&mut write, &msg).await?;
                            tracing::debug!("Sent CapabilitiesChanged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            // Missed transitions: resync the server with the
                            // current snapshot (change events for the missed
                            // steps are lost, but state converges).
                            tracing::warn!("capability transitions lagged by {n}; resyncing");
                            let msg = AgentMessage::CapabilitiesChanged {
                                msg_id: uuid::Uuid::new_v4().to_string(),
                                capabilities: capabilities.effective(),
                                temporary: capabilities.active_grants(),
                                changes: Vec::new(),
                            };
                            send_msg(&mut write, &msg).await?;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // The authority lives as long as the process; a
                            // closed channel means shutdown is underway.
                            return Ok(());
                        }
                    }
                }
                Some(run_result) = events.unlock_result_rx.recv() => {
                    let msg = AgentMessage::UnlockResults {
                        egress_ip: run_result.egress_ip,
                        results: run_result.results,
                        checked_at: run_result.checked_at,
                    };
                    send_msg(&mut write, &msg).await?;
                    tracing::debug!("Sent UnlockResults");
                }
                Some(external_msg) = async {
                    match external_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending::<Option<AgentMessage>>().await,
                    }
                } => {
                    send_msg(&mut write, &external_msg).await?;
                    tracing::debug!("Sent external agent message");
                }
                Some(term_event) = events.term_rx.recv() => {
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
                            runtime.terminal_manager.close(&session_id);
                            AgentMessage::TerminalError {
                                session_id,
                                error: "Session exited".to_string(),
                            }
                        }
                    };
                    send_msg(&mut write, &msg).await?;
                }
                Some(first_result) = events.network_probe_rx.recv() => {
                    let mut results = vec![first_result];
                    // Drain any additional results that arrived at the same time
                    while let Ok(additional) = events.network_probe_rx.try_recv() {
                        results.push(additional);
                    }
                    let count = results.len();
                    let msg = AgentMessage::NetworkProbeResults { results };
                    send_msg(&mut write, &msg).await?;
                    tracing::debug!("Sent NetworkProbeResults ({count} results)");
                }
                Some(file_event) = events.file_rx.recv() => {
                    let msg: AgentMessage = file_event.into();
                    send_msg(&mut write, &msg).await?;
                    tracing::debug!("Sent file event");
                }
                // Docker messages from DockerManager background tasks
                Some(docker_msg) = events.docker_rx.recv() => {
                    send_msg(&mut write, &docker_msg).await?;
                    tracing::debug!("Sent Docker message");
                }
                // Docker housekeeping: stats polling while the daemon is
                // connected, reconnect retry while it is absent.
                tick = runtime.docker.tick() => {
                    let reply = match tick {
                        DockerTick::PollStats => runtime.docker.poll_stats().await,
                        DockerTick::Retry => runtime.docker.retry().await,
                    };
                    if let Some(msg) = reply {
                        send_msg(&mut write, &msg).await?;
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
                server_msg = read.next() => {
                    match server_msg {
                        Some(Ok(Message::Text(text))) => {
                            runtime.handle_server_message(&text, &mut write).await?;
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Server closed connection");
                            runtime.shutdown();
                            return Ok(());
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            tracing::error!("WebSocket error: {e}");
                            runtime.shutdown();
                            return Err(e.into());
                        }
                        None => {
                            tracing::info!("WebSocket stream ended");
                            runtime.shutdown();
                            return Ok(());
                        }
                    }
                }
            }
        }
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
        CapabilitiesConfig, CollectorConfig, FileConfig, IpChangeConfig, LogConfig, SecurityConfig,
        UpgradeConfig,
    };
    use serverbee_common::constants::CapabilityDeniedReason;
    use tokio_tungstenite::tungstenite::http::Response;

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
            capabilities: CapabilitiesConfig::default(),
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
            capabilities: CapabilitiesConfig::default(),
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
            capabilities: CapabilitiesConfig::default(),
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
            capabilities: CapabilitiesConfig::default(),
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

    // ----------------------------------------------------------------------
    // Pure-helper coverage (no I/O, no managers).
    // ----------------------------------------------------------------------

    #[test]
    fn test_build_ws_url_rewrites_schemes_and_trims_trailing_slash() {
        let mk = |url: &str| {
            let c = AgentConfig {
                server_url: url.to_string(),
                token: "t".to_string(),
                enrollment_code: String::new(),
                collector: CollectorConfig::default(),
                log: LogConfig::default(),
                file: FileConfig::default(),
                ip_change: IpChangeConfig::default(),
                upgrade: UpgradeConfig::default(),
                security: SecurityConfig::default(),
                capabilities: CapabilitiesConfig::default(),
            };
            build_ws_url(&c).unwrap()
        };
        assert_eq!(mk("https://example.com"), "wss://example.com/api/agent/ws");
        assert_eq!(mk("http://example.com"), "ws://example.com/api/agent/ws");
        // trailing slash trimmed before appending the path
        assert_eq!(mk("https://example.com/"), "wss://example.com/api/agent/ws");
        // schemeless host gets ws:// prefix
        assert_eq!(mk("example.com:9527"), "ws://example.com:9527/api/agent/ws");
    }

    #[test]
    fn test_apply_jitter_stays_within_bounds_and_has_floor() {
        // 100 samples should always land inside [base - 20%, base + 20%] and
        // never below the 0.5 floor.
        for _ in 0..100 {
            let j = apply_jitter(10);
            assert!((8.0..=12.0).contains(&j), "jitter out of band: {j}");
        }
        // Floor enforced even for base 0.
        for _ in 0..100 {
            let j = apply_jitter(0);
            assert!(j >= 0.5, "jitter below floor: {j}");
        }
    }

    #[test]
    fn test_primary_external_ip_prefers_ipv4_then_ipv6_then_none() {
        // IPv4 wins when both present.
        assert_eq!(
            primary_external_ip(Some("203.0.113.5"), Some("2001:db8::1")),
            Some("203.0.113.5".parse().unwrap())
        );
        // Falls back to IPv6 when IPv4 missing.
        assert_eq!(
            primary_external_ip(None, Some("2001:db8::1")),
            Some("2001:db8::1".parse().unwrap())
        );
        // Unparseable IPv4 falls through to IPv6.
        assert_eq!(
            primary_external_ip(Some("not-an-ip"), Some("2001:db8::1")),
            Some("2001:db8::1".parse().unwrap())
        );
        // Both missing / unparseable -> None.
        assert_eq!(primary_external_ip(None, None), None);
        assert_eq!(primary_external_ip(Some("garbage"), Some("also-bad")), None);
    }

    #[test]
    fn test_collect_interface_ips_is_sorted_and_well_formed() {
        // Real host scan: we can't assert specific addresses, but the result
        // must be name-sorted and every reported interface must carry at least
        // one address.
        let ifaces = collect_interface_ips();
        for w in ifaces.windows(2) {
            assert!(w[0].name <= w[1].name, "interfaces not name-sorted");
        }
        for iface in &ifaces {
            assert!(
                !iface.ipv4.is_empty() || !iface.ipv6.is_empty(),
                "empty interface should not be reported: {}",
                iface.name
            );
        }
    }

    #[test]
    fn test_apply_external_ip_with_none_inputs_passes_through() {
        // Defensive: all-None inputs are a valid pass-through.
        assert_eq!(apply_external_ip(None, None, None), (None, None));
        // External present but no interface IPs: external still applied.
        assert_eq!(
            apply_external_ip(None, None, Some("203.0.113.9".to_string())),
            (Some("203.0.113.9".to_string()), None)
        );
        assert_eq!(
            apply_external_ip(None, None, Some("2001:db8::9".to_string())),
            (None, Some("2001:db8::9".to_string()))
        );
    }

    // ----------------------------------------------------------------------
    // Upgrade progress/failure emitters — pure channel-send helpers.
    // ----------------------------------------------------------------------

    #[tokio::test]
    async fn test_emit_upgrade_progress_sends_progress_message() {
        let (tx, mut rx) = mpsc::channel::<AgentMessage>(4);
        emit_upgrade_progress(
            &tx,
            Some("job-7".to_string()),
            "1.2.3",
            UpgradeStage::Verifying,
        )
        .await;
        let msg = rx.recv().await.expect("progress message expected");
        match msg {
            AgentMessage::UpgradeProgress {
                job_id,
                target_version,
                stage,
                ..
            } => {
                assert_eq!(job_id, Some("job-7".to_string()));
                assert_eq!(target_version, "1.2.3");
                assert_eq!(stage, UpgradeStage::Verifying);
            }
            other => panic!("expected UpgradeProgress, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_emit_upgrade_failure_sends_result_with_error_and_backup() {
        let (tx, mut rx) = mpsc::channel::<AgentMessage>(4);
        emit_upgrade_failure(
            &tx,
            None,
            "2.0.0".to_string(),
            UpgradeStage::Installing,
            "disk full".to_string(),
            Some("/opt/agent.bak".to_string()),
        )
        .await;
        let msg = rx.recv().await.expect("failure message expected");
        match msg {
            AgentMessage::UpgradeResult {
                job_id,
                target_version,
                stage,
                error,
                backup_path,
                ..
            } => {
                assert_eq!(job_id, None);
                assert_eq!(target_version, "2.0.0");
                assert_eq!(stage, UpgradeStage::Installing);
                assert_eq!(error, "disk full");
                assert_eq!(backup_path, Some("/opt/agent.bak".to_string()));
            }
            other => panic!("expected UpgradeResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_emit_upgrade_helpers_swallow_closed_channel() {
        // Dropping the receiver makes send() fail; the helper must log-and-return
        // rather than panic (the send error is intentionally ignored).
        let (tx, rx) = mpsc::channel::<AgentMessage>(1);
        drop(rx);
        // Neither call should panic on the closed channel.
        emit_upgrade_progress(&tx, None, "1.0.0", UpgradeStage::Downloading).await;
        emit_upgrade_failure(
            &tx,
            None,
            "1.0.0".to_string(),
            UpgradeStage::Downloading,
            "x".to_string(),
            None,
        )
        .await;
    }

    // ----------------------------------------------------------------------
    // End-to-end connect/handshake/send/receive/reconnect coverage against a
    // FAKE in-process WebSocket server.
    //
    // These tests drive the *real* connection entry points
    // (`connect_and_report` and `run_with_external`) over a loopback TCP
    // socket so the handshake parse, SystemInfo/Report send loop, the
    // receive/dispatch loop for several ServerMessage variants, server-
    // initiated Close, and the reconnect-with-backoff path all execute
    // against an actual WebSocket — not a mock sink.
    //
    // Everything is bounded by `tokio::time::timeout` so a stuck path fails
    // fast instead of hanging the suite. Network-touching background work
    // (external IP discovery, IP-change polling) is disabled via the config
    // so the only frames the fake server observes come from the code under
    // test.
    // ----------------------------------------------------------------------

    use tokio::net::TcpListener;
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    /// Server-side half of an accepted fake connection.
    type ServerWs = WebSocketStream<tokio::net::TcpStream>;

    /// All capability bits set — every success arm runs.
    const ALL_CAPS: u32 = serverbee_common::constants::CAP_VALID_MASK;

    fn enabled_file_cfg(root: &std::path::Path) -> FileConfig {
        FileConfig {
            enabled: true,
            root_paths: vec![root.to_string_lossy().to_string()],
            ..FileConfig::default()
        }
    }

    /// Build a reporter config that points at the given loopback `ws_addr`
    /// (`host:port`) and disables every network-touching background task so
    /// the fake server only ever sees frames produced by the connect/report
    /// loop itself. `state_dir` is a throwaway temp dir so the capability
    /// grant store/supervisor never touch a real `/var/lib` path.
    fn e2e_config(ws_addr: &std::net::SocketAddr, state_dir: &std::path::Path) -> AgentConfig {
        AgentConfig {
            // build_ws_url prepends ws:// for a schemeless host:port.
            server_url: ws_addr.to_string(),
            token: "e2e-token".to_string(),
            enrollment_code: String::new(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig {
                // Disable the interface-delta poller AND clear the external IP
                // URL list so no public-IP HTTP probe is ever spawned.
                enabled: false,
                external_ip_urls: vec![],
                interval_secs: 3600,
            },
            upgrade: UpgradeConfig::default(),
            security: SecurityConfig::default(),
            capabilities: CapabilitiesConfig {
                state_dir: state_dir.to_string_lossy().to_string(),
                ..CapabilitiesConfig::default()
            },
        }
    }

    /// Bind a loopback listener and return it plus its bound address.
    async fn bind_fake_server() -> (TcpListener, std::net::SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        (listener, addr)
    }

    /// Accept one TCP connection and complete the WebSocket handshake.
    async fn accept_ws(listener: &TcpListener) -> ServerWs {
        let (stream, _) = listener.accept().await.expect("accept");
        tokio_tungstenite::accept_async(stream)
            .await
            .expect("ws handshake")
    }

    /// Send a `Welcome` frame from the server side, advertising the given
    /// report interval (seconds).
    async fn send_welcome(ws: &mut ServerWs, report_interval: u32) {
        let welcome = ServerMessage::Welcome {
            server_id: "fake-server".to_string(),
            protocol_version: serverbee_common::constants::PROTOCOL_VERSION,
            report_interval,
            capabilities: None,
        };
        let json = serde_json::to_string(&welcome).unwrap();
        ws.send(WsMessage::Text(json.into()))
            .await
            .expect("send welcome");
    }

    /// Send any `ServerMessage` as a text frame.
    async fn send_server_msg(ws: &mut ServerWs, msg: &ServerMessage) {
        let json = serde_json::to_string(msg).unwrap();
        ws.send(WsMessage::Text(json.into()))
            .await
            .expect("send server msg");
    }

    /// Send a raw text frame verbatim. Used for hand-rolled JSON (entries whose
    /// constructor shape we don't want to depend on) and for deliberately
    /// malformed / unknown-variant payloads.
    async fn send_raw_text(ws: &mut ServerWs, text: &str) {
        ws.send(WsMessage::Text(text.to_string().into()))
            .await
            .expect("send raw text");
    }

    /// Read text frames from the agent until one decodes into an
    /// `AgentMessage` matching `pred`, returning it. Pings/Pongs/binary frames
    /// and non-matching messages are skipped. Bounded by the outer timeout the
    /// caller wraps this in.
    async fn read_agent_until<F>(ws: &mut ServerWs, mut pred: F) -> AgentMessage
    where
        F: FnMut(&AgentMessage) -> bool,
    {
        loop {
            let frame = ws
                .next()
                .await
                .expect("stream ended before match")
                .expect("ws read error");
            if let WsMessage::Text(text) = frame
                && let Ok(msg) = serde_json::from_str::<AgentMessage>(text.as_str())
                && pred(&msg)
            {
                return msg;
            }
        }
    }

    /// Helper: perform the standard server-side handshake — receive the
    /// agent's first `SystemInfo`, then send an `Ack` for it. Returns the
    /// SystemInfo frame for assertions.
    async fn handshake_collect_system_info(ws: &mut ServerWs) -> AgentMessage {
        let info = read_agent_until(ws, |m| matches!(m, AgentMessage::SystemInfo { .. })).await;
        if let AgentMessage::SystemInfo { msg_id, .. } = &info {
            send_server_msg(
                ws,
                &ServerMessage::Ack {
                    msg_id: msg_id.clone(),
                },
            )
            .await;
        }
        info
    }

    /// Drive `connect_and_report` to completion (or error) with no external
    /// stream, bounded by `dur`. Returns the loop's result; `Elapsed` on the
    /// outer timeout means the connection was still live (never closed).
    async fn run_connect_once(
        reporter: &mut Reporter,
        dur: Duration,
    ) -> Result<anyhow::Result<()>, tokio::time::error::Elapsed> {
        let mut external: Option<mpsc::Receiver<AgentMessage>> = None;
        tokio::time::timeout(dur, reporter.connect_and_report(&mut external)).await
    }

    #[tokio::test]
    async fn test_e2e_handshake_sends_system_info_after_welcome() {
        // Handshake + Welcome parse + first send: after the server sends
        // Welcome, the agent must emit a SystemInfo frame carrying its
        // effective capabilities. Then the server closes and the connect loop
        // returns Ok.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 1).await;
            let info = handshake_collect_system_info(&mut ws).await;
            // Tell the agent to shut the connection down cleanly.
            ws.send(WsMessage::Close(None)).await.ok();
            info
        });

        let connect = run_connect_once(&mut reporter, Duration::from_secs(10)).await;

        let info = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server task timed out")
            .expect("server task panicked");

        // The very first agent frame after Welcome is SystemInfo with our caps.
        match info {
            AgentMessage::SystemInfo {
                agent_local_capabilities,
                ..
            } => {
                assert_eq!(
                    agent_local_capabilities,
                    Some(ALL_CAPS),
                    "SystemInfo must report the agent's effective capabilities"
                );
            }
            other => panic!("expected SystemInfo, got {other:?}"),
        }

        // Server-initiated Close makes connect_and_report return Ok(()).
        let connect = connect.expect("connect loop should finish before the timeout");
        assert!(
            connect.is_ok(),
            "clean server Close should yield Ok: {connect:?}"
        );
    }

    #[tokio::test]
    async fn test_e2e_report_loop_emits_periodic_reports() {
        // Send loop: with a 1s report interval the agent must push at least one
        // Report frame on its own (driven by the collector + interval), which
        // the fake server reads off the wire.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 1).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            // Wait for an unsolicited Report (interval-driven, ~1s).
            let report = tokio::time::timeout(
                Duration::from_secs(8),
                read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Report(_))),
            )
            .await
            .expect("no Report observed within bound");
            ws.send(WsMessage::Close(None)).await.ok();
            report
        });

        let _ = run_connect_once(&mut reporter, Duration::from_secs(12)).await;

        let report = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server task timed out")
            .expect("server task panicked");
        assert!(
            matches!(report, AgentMessage::Report(_)),
            "expected a periodic Report frame"
        );
    }

    #[tokio::test]
    async fn test_e2e_receive_dispatch_ping_pong() {
        // Receive/dispatch loop: a server-sent Ping must round-trip into an
        // agent Pong over the real socket.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(&mut ws, &ServerMessage::Ping).await;
            let pong = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Pong)).await;
            ws.send(WsMessage::Close(None)).await.ok();
            pong
        });

        let _ = run_connect_once(&mut reporter, Duration::from_secs(10)).await;

        let pong = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server task timed out")
            .expect("server task panicked");
        assert!(matches!(pong, AgentMessage::Pong), "Ping must yield Pong");
    }

    #[tokio::test]
    async fn test_e2e_receive_dispatch_exec_returns_task_result() {
        // Receive/dispatch + spawned execution + background-result forwarding:
        // a server Exec drives execute_command and the resulting TaskResult is
        // forwarded back over the WS via the cmd_result channel arm of the
        // select! loop.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::Exec {
                    task_id: "e2e-task".to_string(),
                    command: "printf e2e-out".to_string(),
                    timeout: Some(5),
                },
            )
            .await;
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                read_agent_until(&mut ws, |m| matches!(m, AgentMessage::TaskResult { .. })),
            )
            .await
            .expect("no TaskResult within bound");
            ws.send(WsMessage::Close(None)).await.ok();
            result
        });

        let _ = run_connect_once(&mut reporter, Duration::from_secs(15)).await;

        let result = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server task timed out")
            .expect("server task panicked");
        match result {
            AgentMessage::TaskResult { result, .. } => {
                assert_eq!(result.task_id, "e2e-task");
                assert_eq!(result.exit_code, 0);
                assert!(result.output.contains("e2e-out"));
            }
            other => panic!("expected TaskResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_e2e_ping_tasks_sync_then_close_is_clean() {
        // A non-reply ServerMessage (PingTasksSync with an empty list) is
        // dispatched without producing a WS frame, and the subsequent
        // server-initiated Close still returns Ok — exercising the
        // receive-dispatch path for a "silent" variant plus the Close arm.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(&mut ws, &ServerMessage::PingTasksSync { tasks: vec![] }).await;
            // Then a server Ping to confirm the loop is still alive and
            // processing after the silent sync.
            send_server_msg(&mut ws, &ServerMessage::Ping).await;
            let _ = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Pong)).await;
            ws.send(WsMessage::Close(None)).await.ok();
        });

        let connect = run_connect_once(&mut reporter, Duration::from_secs(10)).await;

        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server task timed out")
            .expect("server task panicked");
        let connect = connect.expect("connect loop should finish before the timeout");
        assert!(
            connect.is_ok(),
            "clean close after sync should be Ok: {connect:?}"
        );
    }

    #[tokio::test]
    async fn test_e2e_server_initiated_close_returns_ok() {
        // Server-initiated Close handling: the server sends Welcome, reads
        // SystemInfo, then immediately closes. connect_and_report must return
        // Ok(()) (the normal-reconnect signal), not an error.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ =
                read_agent_until(&mut ws, |m| matches!(m, AgentMessage::SystemInfo { .. })).await;
            ws.send(WsMessage::Close(None)).await.ok();
            // Drain until the agent's side of the close arrives / stream ends.
            while let Some(Ok(frame)) = ws.next().await {
                if matches!(frame, WsMessage::Close(_)) {
                    break;
                }
            }
        });

        let connect = run_connect_once(&mut reporter, Duration::from_secs(10)).await;
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server task timed out")
            .expect("server task panicked");

        let connect = connect.expect("connect loop should finish before the timeout");
        assert!(
            connect.is_ok(),
            "server Close must yield Ok(()): {connect:?}"
        );
    }

    #[tokio::test]
    async fn test_e2e_connect_failure_to_closed_port_is_error() {
        // Connect-failure path: pointing at a bound-then-closed port makes the
        // TCP/WS connect fail, so connect_and_report returns Err (which the
        // run_with_external loop turns into a backoff+retry).
        let (listener, addr) = bind_fake_server().await;
        // Drop the listener so the port refuses connections.
        drop(listener);
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let connect = run_connect_once(&mut reporter, Duration::from_secs(10)).await;
        let connect = connect.expect("connect should fail fast, not hang");
        assert!(
            connect.is_err(),
            "connecting to a closed port must surface an error"
        );
    }

    #[tokio::test]
    async fn test_e2e_run_loop_reconnects_after_server_close() {
        // Reconnect-with-backoff: run_with_external loops forever. The fake
        // server accepts connection #1, completes the handshake, then closes.
        // After a (jittered, ~0.5-1.2s) backoff the agent must dial again and
        // we observe a *second* accepted+handshaked connection. The whole loop
        // is bounded by an outer timeout that aborts the never-returning task.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        // Server: accept two connections; each time send Welcome, read
        // SystemInfo, then close. Signal each successful handshake.
        let (hs_tx, mut hs_rx) = mpsc::channel::<u32>(2);
        let server = tokio::spawn(async move {
            for n in 1..=2u32 {
                let mut ws = accept_ws(&listener).await;
                send_welcome(&mut ws, 30).await;
                let _ = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::SystemInfo { .. }))
                    .await;
                hs_tx.send(n).await.ok();
                ws.send(WsMessage::Close(None)).await.ok();
                // Give the agent a moment to observe the close before we loop
                // back to accept the reconnect.
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        // Drive the real reconnect loop in the background; abort it once we've
        // confirmed the reconnect so the forever-loop can't outlive the test.
        let agent = tokio::spawn(async move {
            // run_with_external takes the external stream by value; None for tests.
            reporter.run_with_external(None).await;
        });

        // First handshake.
        let first = tokio::time::timeout(Duration::from_secs(8), hs_rx.recv())
            .await
            .expect("first handshake timed out")
            .expect("server dropped before first handshake");
        assert_eq!(first, 1);

        // Second handshake — only reachable if the agent reconnected after the
        // server's first Close (i.e. the run_with_external backoff/retry path).
        let second = tokio::time::timeout(Duration::from_secs(8), hs_rx.recv())
            .await
            .expect("agent did not reconnect within bound")
            .expect("server dropped before reconnect");
        assert_eq!(second, 2, "agent must reconnect after a clean server close");

        agent.abort();
        let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
    }

    // ----------------------------------------------------------------------
    // Additional inbound-dispatch coverage through the REAL select! loop.
    //
    // Each test below drives a single, previously-uncovered `ServerMessage`
    // dispatch arm end-to-end over the fake WS server. We follow the same
    // handshake recipe (accept -> Welcome -> read SystemInfo -> Ack), push one
    // control message, observe (or assert the absence of) the agent's reply on
    // the wire, then Close. Everything stays bounded by tokio::time::timeout so
    // a stuck arm fails fast. Docker-over-socket and wss/TLS arms are skipped:
    // they genuinely require a live daemon or a real certificate.
    // ----------------------------------------------------------------------

    /// Drive `run_connect_once` to completion in the background and join the
    /// server task, both bounded. Returns the server task's value.
    async fn drive_e2e<T>(
        reporter: &mut Reporter,
        server: tokio::task::JoinHandle<T>,
        connect_bound: Duration,
    ) -> T {
        let _ = run_connect_once(reporter, connect_bound).await;
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server task timed out")
            .expect("server task panicked")
    }

    #[tokio::test]
    async fn test_e2e_dispatch_network_probe_sync_is_silent() {
        // NetworkProbeSync with an empty target list is dispatched without
        // producing any WS frame. We confirm liveness afterwards with a
        // Ping->Pong round-trip so the silent arm can't simply have wedged
        // the loop.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::NetworkProbeSync {
                    targets: vec![],
                    interval: 30,
                    packet_count: 3,
                },
            )
            .await;
            // Liveness check: the loop is still processing inbound frames.
            send_server_msg(&mut ws, &ServerMessage::Ping).await;
            let pong = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Pong)).await;
            ws.send(WsMessage::Close(None)).await.ok();
            pong
        });

        let pong = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        assert!(matches!(pong, AgentMessage::Pong));
    }

    #[tokio::test]
    async fn test_e2e_dispatch_ip_quality_sync_and_run_now_are_silent() {
        // IpQualitySync + IpQualityRunNow are accepted silently when the
        // capability is present (default ALL_CAPS includes CAP_IP_QUALITY).
        // No WS frame results; we confirm liveness with Ping->Pong.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::IpQualitySync {
                    services: vec![],
                    interval_hours: 12,
                },
            )
            .await;
            send_server_msg(&mut ws, &ServerMessage::IpQualityRunNow).await;
            send_server_msg(&mut ws, &ServerMessage::Ping).await;
            let pong = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Pong)).await;
            ws.send(WsMessage::Close(None)).await.ok();
            pong
        });

        let pong = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        assert!(matches!(pong, AgentMessage::Pong));
    }

    #[tokio::test]
    async fn test_e2e_dispatch_terminal_open_denied_then_input_resize_close_noop() {
        // TerminalOpen with CAP_TERMINAL revoked: the manager routes to its
        // denied path (no PTY spawned) and emits a TerminalEvent::Error, which
        // the select! loop's term_rx arm forwards over the WS as a
        // TerminalError frame — exercising both the TerminalOpen dispatch arm
        // AND the terminal-event forwarding arm end-to-end. The subsequent
        // input/resize/close ops target a session that was never created, so
        // they are safe no-ops. We assert we see the TerminalError frame and
        // that the loop survives (Ping->Pong afterward).
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_TERMINAL;
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(caps),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::TerminalOpen {
                    session_id: "term-1".to_string(),
                    rows: 24,
                    cols: 80,
                },
            )
            .await;
            // The denied open surfaces a TerminalError over the WS.
            let err = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::TerminalError { session_id, .. } if session_id == "term-1")
            })
            .await;
            // Input/resize/close on the never-opened session are no-ops.
            send_server_msg(
                &mut ws,
                &ServerMessage::TerminalInput {
                    session_id: "term-1".to_string(),
                    data: "aGk=".to_string(),
                },
            )
            .await;
            send_server_msg(
                &mut ws,
                &ServerMessage::TerminalResize {
                    session_id: "term-1".to_string(),
                    rows: 30,
                    cols: 100,
                },
            )
            .await;
            send_server_msg(
                &mut ws,
                &ServerMessage::TerminalClose {
                    session_id: "term-1".to_string(),
                },
            )
            .await;
            send_server_msg(&mut ws, &ServerMessage::Ping).await;
            let pong = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Pong)).await;
            ws.send(WsMessage::Close(None)).await.ok();
            (err, pong)
        });

        let (err, pong) = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        assert!(
            matches!(err, AgentMessage::TerminalError { session_id, .. } if session_id == "term-1"),
            "denied terminal open must surface a TerminalError over the WS"
        );
        assert!(matches!(pong, AgentMessage::Pong));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_e2e_terminal_grant_expiry_tears_down_live_session() {
        // A live PTY session opened under a temporary terminal grant must be
        // torn down when the grant expires: the reconcile step in the
        // transition arm closes the session (TerminalError over the WS) and
        // the CapabilitiesChanged announcement drops the terminal bit.
        use crate::capability_grants::store::GrantRecord;

        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let base = ALL_CAPS & !serverbee_common::constants::CAP_TERMINAL;

        // Seed a grant that expires shortly after the session is opened.
        let config = e2e_config(&addr, tmp.path());
        let grants_path = config.capabilities.grants_path();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut store = crate::capability_grants::store::CapabilityGrantStore::load(&grants_path);
        store.upsert(
            GrantRecord {
                cap: "terminal".into(),
                granted_at: now,
                // Generous window: the PTY must be observably started before
                // the grant expires, even on a loaded CI machine.
                expires_at: now + 5,
                granted_by: "root".into(),
                reason: Some("e2e".into()),
            },
            now,
        );
        store.flush().unwrap();

        let authority = CapabilityAuthority::new(base, grants_path);
        tokio::spawn(Arc::clone(&authority).run(Duration::from_millis(100)));
        let mut reporter = Reporter::new(config, authority);

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let info = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::TerminalOpen {
                    session_id: "term-live".to_string(),
                    rows: 24,
                    cols: 80,
                },
            )
            .await;
            // The grant is active, so the PTY actually starts.
            let _started = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::TerminalStarted { session_id } if session_id == "term-live")
            })
            .await;
            // On expiry the reconcile step closes the session...
            let err = read_agent_until(&mut ws, |m| {
                matches!(
                    m,
                    AgentMessage::TerminalError { session_id, error }
                        if session_id == "term-live" && error.contains("capability")
                )
            })
            .await;
            // ...and the transition is announced without the terminal bit.
            let changed = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::CapabilitiesChanged { .. })
            })
            .await;
            ws.send(WsMessage::Close(None)).await.ok();
            (info, err, changed)
        });

        let (info, _err, changed) = drive_e2e(&mut reporter, server, Duration::from_secs(15)).await;

        // The initial SystemInfo carried the granted terminal capability.
        match info {
            AgentMessage::SystemInfo {
                agent_local_capabilities,
                ..
            } => assert_eq!(
                agent_local_capabilities,
                Some(ALL_CAPS),
                "grant must be folded into the caps reported at connect"
            ),
            other => panic!("expected SystemInfo, got {other:?}"),
        }
        match changed {
            AgentMessage::CapabilitiesChanged {
                capabilities,
                changes,
                ..
            } => {
                assert_eq!(capabilities, base, "expiry must drop the terminal bit");
                assert!(
                    changes.iter().any(|c| c.cap == "terminal"),
                    "the transition must carry the terminal change event"
                );
            }
            other => panic!("expected CapabilitiesChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_e2e_dispatch_blocklist_reset_forwards_ack() {
        // BlocklistReset routes into the FirewallManager and forwards its
        // BlocklistResetAck reply straight back over the WS. On a host without
        // `nft` (macOS CI) the wipe fails but the manager still returns the
        // ack, so the dispatcher always emits exactly one reply frame.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(&mut ws, &ServerMessage::BlocklistReset).await;
            let ack = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::BlocklistResetAck { .. })
            })
            .await;
            ws.send(WsMessage::Close(None)).await.ok();
            ack
        });

        let ack = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        assert!(matches!(ack, AgentMessage::BlocklistResetAck { .. }));
    }

    #[tokio::test]
    async fn test_e2e_dispatch_blocklist_sync_forwards_ack() {
        // BlocklistSync (full-state) routes into the FirewallManager and
        // forwards its BlocklistAck reply over the WS. The apply fails without
        // `nft` but the manager still returns a (failed-state) ack frame.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            // Build a single-entry full-state sync via JSON so we don't depend
            // on the exact BlockEntry constructor shape.
            send_raw_text(
                &mut ws,
                r#"{"type":"blocklist_sync","entries":[{"id":"b1","target":"1.2.3.4/32","family":4}]}"#,
            )
            .await;
            let ack =
                read_agent_until(&mut ws, |m| matches!(m, AgentMessage::BlocklistAck { .. })).await;
            ws.send(WsMessage::Close(None)).await.ok();
            ack
        });

        let ack = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        assert!(matches!(ack, AgentMessage::BlocklistAck { .. }));
    }

    #[tokio::test]
    async fn test_e2e_dispatch_file_list_denied_replies_over_ws() {
        // CAP_FILE revoked: FileList replies with a FileListResult carrying the
        // "disabled" error directly over the WS (the capability-absent branch).
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_FILE;
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(caps),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::FileList {
                    msg_id: "fl-1".to_string(),
                    path: "/tmp".to_string(),
                },
            )
            .await;
            let reply = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::FileListResult { .. })
            })
            .await;
            ws.send(WsMessage::Close(None)).await.ok();
            reply
        });

        let reply = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        match reply {
            AgentMessage::FileListResult { msg_id, error, .. } => {
                assert_eq!(msg_id, "fl-1");
                assert!(
                    error.is_some_and(|e| e.contains("disabled")),
                    "disabled file capability must surface an error"
                );
            }
            other => panic!("expected FileListResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_e2e_dispatch_file_upload_start_acks_over_ws() {
        // File manager enabled with a real temp root: FileUploadStart returns a
        // FileUploadAck at offset 0 over the WS (the success branch of the
        // capability-present + enabled path).
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        // Throwaway state dir for the capability grant store/supervisor.
        let state = tempfile::tempdir().unwrap();
        let mut config = e2e_config(&addr, state.path());
        config.file = enabled_file_cfg(&root);
        let mut reporter = Reporter::new(config, CapabilityAuthority::fixed(ALL_CAPS));

        let dest = root.join("upload.bin");
        let dest_s = dest.to_string_lossy().to_string();
        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::FileUploadStart {
                    transfer_id: "up-1".to_string(),
                    path: dest_s,
                    size: 2,
                },
            )
            .await;
            let ack =
                read_agent_until(&mut ws, |m| matches!(m, AgentMessage::FileUploadAck { .. }))
                    .await;
            ws.send(WsMessage::Close(None)).await.ok();
            ack
        });

        let ack = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        match ack {
            AgentMessage::FileUploadAck {
                transfer_id,
                offset,
            } => {
                assert_eq!(transfer_id, "up-1");
                assert_eq!(offset, 0, "fresh upload starts at offset 0");
            }
            other => panic!("expected FileUploadAck, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_e2e_dispatch_exec_denied_forwards_capability_denied() {
        // CAP_EXEC revoked: Exec is denied and the CapabilityDenied is pushed
        // onto the cmd_result channel, which the select! loop forwards over the
        // WS. This exercises the denied-Exec arm AND the cmd_result_rx
        // forwarding arm end-to-end.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_EXEC;
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(caps),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::Exec {
                    task_id: "denied-exec".to_string(),
                    command: "true".to_string(),
                    timeout: Some(1),
                },
            )
            .await;
            let denied = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::CapabilityDenied { capability, .. } if capability == "exec")
            })
            .await;
            ws.send(WsMessage::Close(None)).await.ok();
            denied
        });

        let denied = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        match denied {
            AgentMessage::CapabilityDenied {
                msg_id,
                capability,
                reason,
                ..
            } => {
                assert_eq!(msg_id, Some("denied-exec".to_string()));
                assert_eq!(capability, "exec");
                assert_eq!(reason, CapabilityDeniedReason::AgentCapabilityDisabled);
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_e2e_dispatch_upgrade_denied_writes_capability_denied_to_ws() {
        // CAP_UPGRADE revoked: Upgrade is denied with a CapabilityDenied written
        // DIRECTLY to the WS sink (not via the cmd channel, unlike Exec).
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_UPGRADE;
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(caps),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_server_msg(
                &mut ws,
                &ServerMessage::Upgrade {
                    version: "9.9.9".to_string(),
                    download_url: String::new(),
                    sha256: String::new(),
                    job_id: Some("up-job".to_string()),
                },
            )
            .await;
            let denied = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::CapabilityDenied { capability, .. } if capability == "upgrade")
            })
            .await;
            ws.send(WsMessage::Close(None)).await.ok();
            denied
        });

        let denied = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        match denied {
            AgentMessage::CapabilityDenied {
                capability, reason, ..
            } => {
                assert_eq!(capability, "upgrade");
                assert_eq!(reason, CapabilityDeniedReason::AgentCapabilityDisabled);
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_e2e_dispatch_traceroute_invalid_target_forwards_error_update() {
        // Capability present but the target fails validation: a completed
        // TracerouteRoundUpdate carrying an error is pushed onto the cmd_result
        // channel and forwarded over the WS. No traceroute subprocess spawns.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_raw_text(
                &mut ws,
                r#"{"type":"traceroute","request_id":"tr-bad","target":"bad target; rm -rf","max_hops":30}"#,
            )
            .await;
            let update = read_agent_until(&mut ws, |m| {
                matches!(m, AgentMessage::TracerouteRoundUpdate { request_id, .. } if request_id == "tr-bad")
            })
            .await;
            ws.send(WsMessage::Close(None)).await.ok();
            update
        });

        let update = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        match update {
            AgentMessage::TracerouteRoundUpdate {
                request_id,
                completed,
                error,
                ..
            } => {
                assert_eq!(request_id, "tr-bad");
                assert!(completed);
                assert!(error.is_some(), "invalid target must carry an error");
            }
            other => panic!("expected TracerouteRoundUpdate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_e2e_dispatch_unparseable_text_is_ignored_then_loop_survives() {
        // A non-JSON / unknown-variant text frame is swallowed by the
        // dispatcher (parse error -> Ok with no output). The loop must keep
        // running, proven by a subsequent Ping->Pong.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            send_raw_text(&mut ws, "definitely not json").await;
            send_raw_text(&mut ws, r#"{"type":"does_not_exist"}"#).await;
            send_server_msg(&mut ws, &ServerMessage::Ping).await;
            let pong = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Pong)).await;
            ws.send(WsMessage::Close(None)).await.ok();
            pong
        });

        let pong = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        assert!(matches!(pong, AgentMessage::Pong));
    }

    #[tokio::test]
    async fn test_e2e_server_ping_frame_round_trips_to_pong_frame() {
        // A WebSocket protocol-level Ping frame (not a ServerMessage::Ping) is
        // answered by the dedicated `Message::Ping(data) => Pong(data)` arm of
        // the select! loop. tokio-tungstenite may auto-respond to control
        // frames, so we don't assert on the Pong wire frame directly; instead
        // we send a Ping frame and then confirm the loop is still alive and
        // dispatching application messages via a ServerMessage::Ping->Pong.
        let (listener, addr) = bind_fake_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let mut reporter = Reporter::new(
            e2e_config(&addr, tmp.path()),
            CapabilityAuthority::fixed(ALL_CAPS),
        );

        let server = tokio::spawn(async move {
            let mut ws = accept_ws(&listener).await;
            send_welcome(&mut ws, 30).await;
            let _ = handshake_collect_system_info(&mut ws).await;
            ws.send(WsMessage::Ping(vec![1, 2, 3].into())).await.ok();
            send_server_msg(&mut ws, &ServerMessage::Ping).await;
            let pong = read_agent_until(&mut ws, |m| matches!(m, AgentMessage::Pong)).await;
            ws.send(WsMessage::Close(None)).await.ok();
            pong
        });

        let pong = drive_e2e(&mut reporter, server, Duration::from_secs(10)).await;
        assert!(matches!(pong, AgentMessage::Pong));
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
            emit_upgrade_failure(
                &tx,
                job_id.clone(),
                version.to_string(),
                $stage,
                msg.clone(),
                None,
            )
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
    let (binary_url, checksums_url) = match derive_urls(&upgrade_cfg.release_repo_url, version) {
        Ok(v) => v,
        Err(e) => fail!(UpgradeStage::Downloading, format!("derive url: {e}")),
    };

    // 4. 专用 client
    let client = match build_upgrade_client(spki.as_deref()) {
        Ok(c) => c,
        Err(e) => fail!(UpgradeStage::Downloading, format!("tls client: {e}")),
    };

    tracing::info!("Downloading agent v{version} from pinned source {binary_url}");

    // 5. 拉 sha256sums.txt
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
    tracing::info!("Checksum verified against pinned sha256sums.txt");

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
