//! Connection-scoped command runtime.
//!
//! One WebSocket connection owns one [`ConnectionRuntime`]: every manager
//! and outbound channel sender that executing a `ServerMessage` can touch
//! lives here, so the dispatcher is a method on the state it drives instead
//! of a free function threading a dozen borrows. `mod.rs` keeps transport
//! concerns only (connect/backoff, the select! loop, report and IP timers);
//! file replies stay in `file_ops`, framing in `wire`, and the Docker
//! availability state machine in `docker_subsystem`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::SinkExt;
use serverbee_common::constants::{DEFAULT_COMMAND_TIMEOUT_SECS, MAX_TASK_OUTPUT_SIZE};
use serverbee_common::protocol::{AgentMessage, ServerMessage, UpgradeStage};
use serverbee_common::types::{NetworkProbeResultData, PingResult};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::docker_subsystem::DockerSubsystem;
use super::wire::send_msg;
use super::{emit_upgrade_failure, file_ops, perform_upgrade};
use crate::capability_grants::CapabilityAuthority;
use crate::config::{AgentConfig, UpgradeConfig};
use crate::file_manager::{FileEvent, FileManager};
use crate::firewall::FirewallManager;
use crate::ip_quality::{RunResult, UnlockChecker};
use crate::network_prober::NetworkProber;
use crate::pinger::PingManager;
use crate::terminal::{TerminalEvent, TerminalManager};

/// Process-wide single-flight latch for binary self-upgrade. A duplicate
/// Upgrade command must be rejected without disturbing the running one.
static UPGRADE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Receiver halves for every event stream the runtime's managers emit. The
/// select! loop in `mod.rs` drains these; the matching senders live inside
/// [`ConnectionRuntime`] so a handler can never outlive its channel.
pub(super) struct ConnectionEvents {
    pub(super) ping_rx: mpsc::Receiver<PingResult>,
    pub(super) term_rx: mpsc::Receiver<TerminalEvent>,
    pub(super) network_probe_rx: mpsc::Receiver<NetworkProbeResultData>,
    pub(super) file_rx: mpsc::Receiver<FileEvent>,
    pub(super) unlock_result_rx: mpsc::Receiver<RunResult>,
    pub(super) docker_rx: mpsc::Receiver<AgentMessage>,
    pub(super) cmd_result_rx: mpsc::Receiver<AgentMessage>,
}

/// Everything a single agent connection needs to execute server commands.
pub(super) struct ConnectionRuntime {
    capabilities: Arc<CapabilityAuthority>,
    ping_manager: PingManager,
    pub(super) terminal_manager: TerminalManager,
    network_prober: NetworkProber,
    file_manager: FileManager,
    pub(super) unlock_checker: UnlockChecker,
    pub(super) cmd_result_tx: mpsc::Sender<AgentMessage>,
    file_tx: mpsc::Sender<FileEvent>,
    pub(super) docker: DockerSubsystem,
    upgrade_cfg: UpgradeConfig,
    firewall_manager: Arc<FirewallManager>,
}

impl ConnectionRuntime {
    /// Build the runtime and hand back the event receivers the select! loop
    /// drains. Docker starts absent; call [`Self::probe_docker`] before
    /// advertising features.
    pub(super) fn new(
        config: &AgentConfig,
        capabilities: Arc<CapabilityAuthority>,
        firewall_manager: Arc<FirewallManager>,
    ) -> (Self, ConnectionEvents) {
        let (ping_tx, ping_rx) = mpsc::channel(256);
        let ping_manager = PingManager::new(ping_tx, Arc::clone(&capabilities));

        let (term_tx, term_rx) = mpsc::channel(256);
        let terminal_manager = TerminalManager::new(term_tx, Arc::clone(&capabilities));

        let (network_probe_tx, network_probe_rx) = mpsc::channel::<NetworkProbeResultData>(256);
        let network_prober = NetworkProber::new(network_probe_tx, Arc::clone(&capabilities));

        let (unlock_result_tx, unlock_result_rx) = mpsc::channel::<RunResult>(8);
        let unlock_checker = UnlockChecker::new(Arc::clone(&capabilities), unlock_result_tx);

        let (file_tx, file_rx) = mpsc::channel::<FileEvent>(16);
        let file_manager = FileManager::new(config.file.clone(), Arc::clone(&capabilities));

        let (docker_tx, docker_rx) = mpsc::channel::<AgentMessage>(256);
        let docker = DockerSubsystem::new(docker_tx, Arc::clone(&capabilities));

        let (cmd_result_tx, cmd_result_rx) = mpsc::channel::<AgentMessage>(32);

        let runtime = Self {
            capabilities,
            ping_manager,
            terminal_manager,
            network_prober,
            file_manager,
            unlock_checker,
            cmd_result_tx,
            file_tx,
            docker,
            upgrade_cfg: config.upgrade.clone(),
            firewall_manager,
        };
        let events = ConnectionEvents {
            ping_rx,
            term_rx,
            network_probe_rx,
            file_rx,
            unlock_result_rx,
            docker_rx,
            cmd_result_rx,
        };
        (runtime, events)
    }

    /// Tear down every in-flight resource this connection owns. Called on all
    /// connection-exit paths (server close, WS error, stream end).
    pub(super) fn shutdown(&mut self) {
        self.ping_manager.stop_all();
        self.terminal_manager.close_all();
        self.network_prober.stop_all();
        self.unlock_checker.stop();
        self.file_manager.cancel_all_transfers();
        self.docker.cleanup();
    }

    /// Parse and execute one server text frame against this connection's
    /// state. Unparseable frames are logged and swallowed — a protocol
    /// mismatch must never tear the connection down.
    pub(super) async fn handle_server_message<S>(
        &mut self,
        text: &str,
        write: &mut S,
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
            ServerMessage::Ping => {
                send_msg(write, &AgentMessage::Pong).await?;
                tracing::debug!("Responded to Ping with Pong");
            }
            ServerMessage::Exec {
                task_id,
                command,
                timeout,
            } => {
                if !self.capabilities.has(CAP_EXEC) {
                    tracing::warn!("Exec denied: capability disabled (task_id={task_id})");
                    spawn_capability_denied(&self.cmd_result_tx, Some(task_id), "exec");
                    return Ok(());
                }
                tracing::info!("Executing command (task_id={task_id}): {command}");
                let tx = self.cmd_result_tx.clone();
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
                self.ping_manager.sync(tasks);
            }
            ServerMessage::TerminalOpen {
                session_id,
                rows,
                cols,
            } => {
                tracing::info!("Opening terminal session {session_id} ({cols}x{rows})");
                self.terminal_manager.open(session_id, rows, cols);
            }
            ServerMessage::TerminalInput { session_id, data } => {
                self.terminal_manager.write_input(&session_id, &data);
            }
            ServerMessage::TerminalResize {
                session_id,
                rows,
                cols,
            } => {
                tracing::debug!("Resizing terminal {session_id} to {cols}x{rows}");
                self.terminal_manager.resize(&session_id, rows, cols);
            }
            ServerMessage::TerminalClose { session_id } => {
                tracing::debug!("Closing terminal session {session_id}");
                self.terminal_manager.close(&session_id);
            }
            ServerMessage::Upgrade {
                version, job_id, ..
            } => {
                if !self.capabilities.has(CAP_UPGRADE) {
                    tracing::warn!("Upgrade denied: capability disabled");
                    send_msg(write, &capability_denied_msg(None, "upgrade")).await?;
                    return Ok(());
                }

                if UPGRADE_IN_PROGRESS
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    let tx = self.cmd_result_tx.clone();
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
                    return Ok(());
                }

                tracing::info!("Upgrade requested: v{version} (pinned source)");
                let upgrade_cfg = self.upgrade_cfg.clone();
                let tx = self.cmd_result_tx.clone();
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
                self.network_prober.sync(targets, interval, packet_count);
            }
            ServerMessage::Traceroute {
                request_id,
                target,
                max_hops,
                protocol,
            } => {
                if !self.capabilities.has(CAP_PING_ICMP) {
                    tracing::warn!(
                        "Traceroute denied: capability disabled (request_id={request_id})"
                    );
                    spawn_capability_denied(&self.cmd_result_tx, Some(request_id), "ping_icmp");
                    return Ok(());
                }

                // Input validation: target must be domain or IP only.
                if !crate::traceroute::is_valid_traceroute_target(&target) {
                    tracing::warn!(
                        "Traceroute rejected: invalid target '{target}' (request_id={request_id})"
                    );
                    let tx = self.cmd_result_tx.clone();
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
                    return Ok(());
                }

                let proto = protocol.unwrap_or(serverbee_common::protocol::TraceProtocol::Icmp);
                tracing::info!(
                    "Executing traceroute to {target} (max_hops={max_hops}, request_id={request_id}, protocol={proto:?})"
                );
                crate::traceroute::spawn_traceroute(
                    request_id,
                    target,
                    max_hops,
                    proto,
                    self.cmd_result_tx.clone(),
                );
            }
            // --- File management messages --- (gate + reply shapes live in file_ops)
            msg @ (ServerMessage::FileList { .. }
            | ServerMessage::FileStat { .. }
            | ServerMessage::FileRead { .. }
            | ServerMessage::FileWrite { .. }
            | ServerMessage::FileDelete { .. }
            | ServerMessage::FileMkdir { .. }
            | ServerMessage::FileMove { .. }
            | ServerMessage::FileDownloadStart { .. }
            | ServerMessage::FileDownloadCancel { .. }
            | ServerMessage::FileUploadStart { .. }
            | ServerMessage::FileUploadChunk { .. }
            | ServerMessage::FileUploadEnd { .. }) => {
                file_ops::handle_file_message(
                    msg,
                    write,
                    &self.file_manager,
                    &self.file_tx,
                    &self.capabilities,
                )
                .await?;
            }
            // --- Docker messages --- (availability FSM + replies live in
            // docker_subsystem; the reply is whatever the transition emits)
            msg @ (ServerMessage::DockerStartStats { .. }
            | ServerMessage::DockerStopStats
            | ServerMessage::DockerListContainers { .. }
            | ServerMessage::DockerLogsStart { .. }
            | ServerMessage::DockerLogsStop { .. }
            | ServerMessage::DockerEventsStart
            | ServerMessage::DockerEventsStop
            | ServerMessage::DockerContainerAction { .. }
            | ServerMessage::DockerGetInfo { .. }
            | ServerMessage::DockerListNetworks { .. }
            | ServerMessage::DockerListVolumes { .. }) => {
                if let Some(reply) = self.docker.handle(msg).await {
                    send_msg(write, &reply).await?;
                }
            }
            // Firewall blocklist variants — dispatched to the FirewallManager
            // state machine; any returned ack is sent straight back over the
            // WebSocket.
            //
            // The mutating variants (Sync/Add/Remove) enforce CAP_FIREWALL_BLOCK
            // on the agent's own host, mirroring the capability gates on Exec /
            // File / Traceroute etc. — the server is not the only trust boundary.
            // BlocklistReset is deliberately *not* gated: it wipes ServerBee's
            // own nft table (cleanup / disable path) and must stay reachable even
            // after the capability is revoked, so a denied agent can still be
            // cleaned up.
            msg @ (ServerMessage::BlocklistSync { .. }
            | ServerMessage::BlocklistAdd { .. }
            | ServerMessage::BlocklistRemove { .. }
            | ServerMessage::BlocklistReset) => {
                let is_reset = matches!(msg, ServerMessage::BlocklistReset);
                if !is_reset && !self.capabilities.has(CAP_FIREWALL_BLOCK) {
                    tracing::warn!(
                        "Firewall blocklist mutation denied: CAP_FIREWALL_BLOCK not effective — ignoring"
                    );
                } else if let Some(reply) = self.firewall_manager.handle(msg).await {
                    send_msg(write, &reply).await?;
                    tracing::debug!("Sent firewall blocklist ack");
                }
            }
            ServerMessage::IpQualitySync {
                services,
                interval_hours,
            } => {
                if self.capabilities.has(CAP_IP_QUALITY) {
                    tracing::info!(
                        "Received IpQualitySync: {} services, interval={}h",
                        services.len(),
                        interval_hours
                    );
                    self.unlock_checker.sync(services, interval_hours).await;
                } else {
                    tracing::debug!(
                        "IpQualitySync received but CAP_IP_QUALITY not effective — ignoring"
                    );
                }
            }
            ServerMessage::IpQualityRunNow => {
                if self.capabilities.has(CAP_IP_QUALITY) {
                    tracing::info!("Received IpQualityRunNow");
                    self.unlock_checker.run_now();
                } else {
                    tracing::debug!(
                        "IpQualityRunNow received but CAP_IP_QUALITY not effective — ignoring"
                    );
                }
            }
        }

        Ok(())
    }
}

/// Build the standard agent-local capability denial message.
fn capability_denied_msg(msg_id: Option<String>, capability: &str) -> AgentMessage {
    AgentMessage::CapabilityDenied {
        msg_id,
        session_id: None,
        capability: capability.to_string(),
        reason: serverbee_common::constants::CapabilityDeniedReason::AgentCapabilityDisabled,
    }
}

/// Queue a capability denial onto the outbound channel without blocking the
/// read loop.
fn spawn_capability_denied(
    tx: &mpsc::Sender<AgentMessage>,
    msg_id: Option<String>,
    capability: &str,
) {
    let denied = capability_denied_msg(msg_id, capability);
    let tx = tx.clone();
    tokio::spawn(async move {
        let _ = tx.send(denied).await;
    });
}

async fn execute_command(
    task_id: &str,
    command: &str,
    timeout: Option<u32>,
) -> serverbee_common::types::TaskResult {
    let timeout_secs = timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT_SECS);

    let mut process = tokio::process::Command::new("sh");
    process
        .arg("-c")
        .arg(command)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        process.as_std_mut().process_group(0);
    }

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return command_error(task_id, format!("Failed to execute command: {error}"));
        }
    };
    let process_group_id = child.id();
    let mut stdout_task = child
        .stdout
        .take()
        .map(|stdout| tokio::spawn(read_pipe(stdout)));
    let mut stderr_task = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_pipe(stderr)));

    let execution = async {
        let status = child.wait().await?;
        let stdout = collect_pipe(stdout_task.take()).await;
        let stderr = collect_pipe(stderr_task.take()).await;
        Ok::<_, std::io::Error>((status, stdout, stderr))
    };
    match tokio::time::timeout(Duration::from_secs(timeout_secs as u64), execution).await {
        Ok(Ok((status, stdout, stderr))) => {
            let mut combined = String::from_utf8_lossy(&stdout).to_string();
            let stderr = String::from_utf8_lossy(&stderr);
            if !stderr.is_empty() {
                combined.push('\n');
                combined.push_str(&stderr);
            }
            if combined.len() > MAX_TASK_OUTPUT_SIZE {
                let mut boundary = MAX_TASK_OUTPUT_SIZE;
                while !combined.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                combined.truncate(boundary);
                combined.push_str("\n... (output truncated)");
            }
            serverbee_common::types::TaskResult {
                task_id: task_id.to_string(),
                output: combined,
                exit_code: status.code().unwrap_or(-1),
            }
        }
        Ok(Err(error)) => {
            terminate_command_tree(&mut child, process_group_id).await;
            command_error(task_id, format!("Failed to execute command: {error}"))
        }
        Err(_) => {
            terminate_command_tree(&mut child, process_group_id).await;
            let _ = collect_pipe(stdout_task.take()).await;
            let _ = collect_pipe(stderr_task.take()).await;
            command_error(task_id, format!("Command timed out after {timeout_secs}s"))
        }
    }
}

fn command_error(task_id: &str, output: String) -> serverbee_common::types::TaskResult {
    serverbee_common::types::TaskResult {
        task_id: task_id.to_string(),
        output,
        exit_code: -1,
    }
}

async fn read_pipe<R>(mut pipe: R) -> std::io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut retained = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = pipe.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let remaining = (MAX_TASK_OUTPUT_SIZE + 1).saturating_sub(retained.len());
        retained.extend_from_slice(&chunk[..read.min(remaining)]);
    }
    Ok(retained)
}

async fn collect_pipe(task: Option<tokio::task::JoinHandle<std::io::Result<Vec<u8>>>>) -> Vec<u8> {
    match task {
        Some(task) => match task.await {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "Failed to read command output pipe");
                Vec::new()
            }
            Err(error) => {
                tracing::warn!(error = %error, "Command output reader task failed");
                Vec::new()
            }
        },
        None => Vec::new(),
    }
}

async fn terminate_command_tree(child: &mut tokio::process::Child, process_group_id: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = process_group_id.and_then(|pid| i32::try_from(pid).ok()) {
        // The shell is the process-group leader, so a negative PID reaches the
        // shell and every descendant it spawned.
        let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                tracing::warn!(pid, error = %error, "Failed to kill command process group");
            }
        }
    }

    #[cfg(windows)]
    if let Some(pid) = process_group_id {
        let result = tokio::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .await;
        if let Err(error) = result {
            tracing::warn!(pid, error = %error, "Failed to run taskkill for command tree");
        }
    }

    if let Err(error) = child.start_kill()
        && error.kind() != std::io::ErrorKind::InvalidInput
    {
        tracing::warn!(error = %error, "Failed to kill command process");
    }
    let _ = child.wait().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CapabilitiesConfig, CollectorConfig, FileConfig, IpChangeConfig, LogConfig, SecurityConfig,
        UpgradeConfig,
    };
    use crate::firewall::nft::CliNftExecutor;
    use serverbee_common::constants::CapabilityDeniedReason;

    // ----------------------------------------------------------------------
    // `execute_command` — pure-ish process helper. These shell out to `sh`,
    // which is always present on macOS/Linux CI, so they remain deterministic.
    // ----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_command_captures_stdout_and_zero_exit() {
        // Deterministic stdout, exit 0.
        let r = execute_command("t-ok", "printf hello", Some(5)).await;
        assert_eq!(r.task_id, "t-ok");
        assert_eq!(r.exit_code, 0);
        assert!(r.output.contains("hello"));
    }

    #[tokio::test]
    async fn test_execute_command_nonzero_exit_and_stderr_appended() {
        // `sh -c 'exit 3'` yields exit_code 3; stderr is folded into output.
        let r = execute_command("t-fail", "echo oops 1>&2; exit 3", Some(5)).await;
        assert_eq!(r.exit_code, 3);
        assert!(
            r.output.contains("oops"),
            "stderr must be appended to output"
        );
    }

    #[tokio::test]
    async fn test_execute_command_truncates_large_output() {
        // Emit more than MAX_TASK_OUTPUT_SIZE bytes; the helper must cap and
        // append the truncation marker. `yes | head -c N` is portable.
        let cmd = format!("yes A | head -c {}", MAX_TASK_OUTPUT_SIZE + 5000);
        let r = execute_command("t-big", &cmd, Some(10)).await;
        assert_eq!(r.exit_code, 0);
        assert!(
            r.output.ends_with("\n... (output truncated)"),
            "oversized output must carry the truncation marker"
        );
        assert!(
            r.output.len() <= MAX_TASK_OUTPUT_SIZE + "\n... (output truncated)".len(),
            "truncated output must respect the cap"
        );
    }

    #[tokio::test]
    async fn test_execute_command_times_out_with_negative_exit() {
        // A 2s sleep against a 1s timeout must surface the timeout branch.
        let r = execute_command("t-timeout", "sleep 2", Some(1)).await;
        assert_eq!(r.exit_code, -1);
        assert!(
            r.output.contains("timed out"),
            "timeout branch must report it"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execute_command_timeout_kills_descendant_processes() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let pid_path = temp.path().join("child.pid");
        let command = format!(
            "sleep 30 & child=$!; echo $child > '{}'; printf launched",
            pid_path.display()
        );

        let result = execute_command("t-tree-timeout", &command, Some(1)).await;
        assert_eq!(result.exit_code, -1);
        assert!(result.output.contains("timed out"));

        let pid: i32 = std::fs::read_to_string(&pid_path)
            .expect("child pid should be recorded before timeout")
            .trim()
            .parse()
            .expect("child pid should be numeric");
        let mut gone = false;
        for _ in 0..20 {
            let exists = unsafe { libc::kill(pid, 0) } == 0;
            if !exists && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !gone {
            let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        assert!(gone, "timed-out command descendant {pid} is still alive");
    }

    // ----------------------------------------------------------------------
    // `handle_server_message` dispatcher coverage via a mock sink.
    //
    // The dispatcher is generic over the WS sink (`S: SinkExt<Message,...>`),
    // so we drive it with an in-memory recording sink instead of a real
    // WebSocket. A `Harness` owns the runtime plus the receiver ends of its
    // event channels so spawned senders never error.
    // ----------------------------------------------------------------------

    /// In-memory sink that records every `Message` written to it. All poll_*
    /// hooks succeed immediately; `start_send` just pushes into a shared Vec.
    #[derive(Clone)]
    struct RecordingSink {
        sent: Arc<std::sync::Mutex<Vec<Message>>>,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                sent: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        /// All recorded messages decoded into `AgentMessage` (text frames only).
        fn agent_messages(&self) -> Vec<AgentMessage> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .filter_map(|m| match m {
                    Message::Text(t) => serde_json::from_str::<AgentMessage>(t.as_str()).ok(),
                    _ => None,
                })
                .collect()
        }

        fn sent_count(&self) -> usize {
            self.sent.lock().unwrap().len()
        }
    }

    impl futures_util::Sink<Message> for RecordingSink {
        type Error = tokio_tungstenite::tungstenite::Error;

        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn start_send(self: std::pin::Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            self.sent.lock().unwrap().push(item);
            Ok(())
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    /// Owns the runtime plus the receiver halves of its event channels,
    /// keeping them alive so background senders never see a closed channel.
    struct Harness {
        runtime: ConnectionRuntime,
        events: ConnectionEvents,
    }

    impl Harness {
        /// Build a harness with the given capability bits. `file_cfg` lets
        /// individual tests enable the file manager with a temp root.
        fn new(caps: u32, file_cfg: FileConfig) -> Self {
            let config = AgentConfig {
                server_url: "http://127.0.0.1:9527".to_string(),
                token: "t".to_string(),
                enrollment_code: String::new(),
                collector: CollectorConfig::default(),
                log: LogConfig::default(),
                file: file_cfg,
                ip_change: IpChangeConfig::default(),
                upgrade: UpgradeConfig::default(),
                security: SecurityConfig::default(),
                capabilities: CapabilitiesConfig::default(),
            };
            let capabilities = CapabilityAuthority::fixed(caps);
            let firewall_manager = Arc::new(FirewallManager::new(Arc::new(CliNftExecutor)));
            let (runtime, events) = ConnectionRuntime::new(&config, capabilities, firewall_manager);
            Self { runtime, events }
        }

        /// Dispatch a single `ServerMessage` (as JSON) through the method.
        async fn dispatch(&mut self, text: &str, sink: &mut RecordingSink) -> anyhow::Result<()> {
            self.runtime.handle_server_message(text, sink).await
        }
    }

    /// All capability bits set — every success arm runs.
    const ALL_CAPS: u32 = serverbee_common::constants::CAP_VALID_MASK;

    fn enabled_file_cfg(root: &std::path::Path) -> FileConfig {
        FileConfig {
            enabled: true,
            root_paths: vec![root.to_string_lossy().to_string()],
            ..FileConfig::default()
        }
    }

    #[tokio::test]
    async fn test_blocklist_mutation_denied_without_firewall_capability() {
        use serverbee_common::constants::CAP_FIREWALL_BLOCK;
        // All caps except firewall block — simulates a revoked capability.
        let caps = ALL_CAPS & !CAP_FIREWALL_BLOCK;
        let mut h = Harness::new(caps, FileConfig::default());
        let mut sink = RecordingSink::new();

        // A mutating variant must be dropped before reaching the firewall
        // manager, so nothing is written back and no nft command runs.
        h.dispatch(r#"{"type":"blocklist_remove","id":"x"}"#, &mut sink)
            .await
            .unwrap();
        assert_eq!(
            sink.sent_count(),
            0,
            "blocklist mutation must be ignored when CAP_FIREWALL_BLOCK is off"
        );
    }

    #[tokio::test]
    async fn test_dispatch_unparseable_text_is_ignored() {
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        // Not valid JSON for ServerMessage — must be swallowed as Ok with no output.
        h.dispatch("this is not json", &mut sink).await.unwrap();
        h.dispatch(r#"{"type":"nonexistent_variant"}"#, &mut sink)
            .await
            .unwrap();
        assert_eq!(
            sink.sent_count(),
            0,
            "unparseable input must not emit anything"
        );
    }

    #[tokio::test]
    async fn test_dispatch_ping_responds_with_pong() {
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(r#"{"type":"ping"}"#, &mut sink).await.unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(msgs[0], AgentMessage::Pong));
    }

    #[tokio::test]
    async fn test_dispatch_ack_and_welcome_are_noops() {
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(r#"{"type":"ack","msg_id":"m1"}"#, &mut sink)
            .await
            .unwrap();
        h.dispatch(
            r#"{"type":"welcome","server_id":"s","protocol_version":1,"report_interval":3}"#,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(
            sink.sent_count(),
            0,
            "ack/welcome must not write to the sink"
        );
    }

    #[tokio::test]
    async fn test_dispatch_exec_denied_when_capability_absent() {
        // CAP_EXEC missing -> a CapabilityDenied is pushed onto cmd_result_tx
        // (NOT the sink). We drain the channel to assert.
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_EXEC;
        let mut h = Harness::new(caps, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"exec","task_id":"task-42","command":"true","timeout":1}"#,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(
            sink.sent_count(),
            0,
            "denied exec writes to channel, not sink"
        );
        let denied = h
            .events
            .cmd_result_rx
            .recv()
            .await
            .expect("denied msg expected");
        match denied {
            AgentMessage::CapabilityDenied {
                msg_id,
                capability,
                reason,
                ..
            } => {
                assert_eq!(msg_id, Some("task-42".to_string()));
                assert_eq!(capability, "exec");
                assert_eq!(reason, CapabilityDeniedReason::AgentCapabilityDisabled);
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_dispatch_exec_allowed_runs_and_emits_task_result() {
        // `true` is a deterministic, always-present POSIX builtin/command.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"exec","task_id":"task-ok","command":"true","timeout":5}"#,
            &mut sink,
        )
        .await
        .unwrap();
        // The execution is spawned; await its TaskResult on the channel.
        let result = tokio::time::timeout(Duration::from_secs(10), h.events.cmd_result_rx.recv())
            .await
            .expect("task did not complete in time")
            .expect("TaskResult expected");
        match result {
            AgentMessage::TaskResult { result, .. } => {
                assert_eq!(result.task_id, "task-ok");
                assert_eq!(result.exit_code, 0);
            }
            other => panic!("expected TaskResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_dispatch_ping_tasks_sync_is_accepted() {
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        // Empty task list — sync clears the manager; dispatcher returns Ok with
        // no WS output.
        h.dispatch(r#"{"type":"ping_tasks_sync","tasks":[]}"#, &mut sink)
            .await
            .unwrap();
        assert_eq!(sink.sent_count(), 0);
    }

    #[tokio::test]
    async fn test_dispatch_network_probe_sync_is_accepted() {
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"network_probe_sync","targets":[],"interval":30,"packet_count":3}"#,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.sent_count(), 0);
    }

    #[tokio::test]
    async fn test_dispatch_terminal_lifecycle_without_capability() {
        // CAP_TERMINAL off: open() routes to the denied event (no PTY spawned),
        // and input/resize/close on a missing session are safe no-ops. The
        // dispatcher must return Ok for each and never touch the sink.
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_TERMINAL;
        let mut h = Harness::new(caps, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"terminal_open","session_id":"s1","rows":24,"cols":80}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"terminal_input","session_id":"s1","data":"aGk="}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"terminal_resize","session_id":"s1","rows":30,"cols":100}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(r#"{"type":"terminal_close","session_id":"s1"}"#, &mut sink)
            .await
            .unwrap();
        assert_eq!(
            sink.sent_count(),
            0,
            "terminal control writes nothing to the WS sink"
        );
    }

    #[tokio::test]
    async fn test_dispatch_upgrade_denied_when_capability_absent() {
        // CAP_UPGRADE off -> denied is written DIRECTLY to the sink (not the
        // channel), unlike Exec.
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_UPGRADE;
        let mut h = Harness::new(caps, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"upgrade","version":"9.9.9","job_id":"j1"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            AgentMessage::CapabilityDenied {
                capability, reason, ..
            } => {
                assert_eq!(capability, "upgrade");
                assert_eq!(*reason, CapabilityDeniedReason::AgentCapabilityDisabled);
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_dispatch_traceroute_denied_when_capability_absent() {
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_PING_ICMP;
        let mut h = Harness::new(caps, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"traceroute","request_id":"r1","target":"example.com","max_hops":30}"#,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(
            sink.sent_count(),
            0,
            "denied traceroute goes to the channel"
        );
        let denied = h
            .events
            .cmd_result_rx
            .recv()
            .await
            .expect("denied expected");
        match denied {
            AgentMessage::CapabilityDenied {
                msg_id,
                capability,
                reason,
                ..
            } => {
                assert_eq!(msg_id, Some("r1".to_string()));
                assert_eq!(capability, "ping_icmp");
                assert_eq!(reason, CapabilityDeniedReason::AgentCapabilityDisabled);
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_dispatch_traceroute_invalid_target_is_rejected() {
        // Capability present but target fails validation -> a completed
        // TracerouteRoundUpdate with an error is emitted on the channel. No
        // real traceroute subprocess is spawned.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"traceroute","request_id":"r2","target":"bad target with spaces; rm -rf","max_hops":30}"#,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.sent_count(), 0);
        let msg = h
            .events
            .cmd_result_rx
            .recv()
            .await
            .expect("update expected");
        match msg {
            AgentMessage::TracerouteRoundUpdate {
                request_id,
                completed,
                error,
                ..
            } => {
                assert_eq!(request_id, "r2");
                assert!(completed);
                assert!(error.is_some(), "invalid target must carry an error");
            }
            other => panic!("expected TracerouteRoundUpdate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_dispatch_file_ops_denied_when_capability_absent() {
        // CAP_FILE off -> each file op replies with a disabled error frame on
        // the sink (capability-absent branch).
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_FILE;
        let mut h = Harness::new(caps, FileConfig::default());
        let mut sink = RecordingSink::new();

        h.dispatch(
            r#"{"type":"file_list","msg_id":"m1","path":"/tmp"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_stat","msg_id":"m2","path":"/tmp"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_read","msg_id":"m3","path":"/tmp/x","max_size":1024}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_write","msg_id":"m4","path":"/tmp/x","content":"aGk="}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_delete","msg_id":"m5","path":"/tmp/x","recursive":false}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_mkdir","msg_id":"m6","path":"/tmp/d"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_move","msg_id":"m7","from":"/tmp/a","to":"/tmp/b"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_download_start","transfer_id":"t1","path":"/tmp/x"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_upload_start","transfer_id":"t2","path":"/tmp/x","size":4}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_upload_chunk","transfer_id":"t3","offset":0,"data":"aGk="}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_upload_end","transfer_id":"t4"}"#,
            &mut sink,
        )
        .await
        .unwrap();

        let msgs = sink.agent_messages();
        // 11 dispatches each produce exactly one response frame.
        assert_eq!(msgs.len(), 11, "each denied file op emits one frame");
        // Spot-check representative variants carry the disabled error.
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileListResult { error: Some(e), .. } if e.contains("disabled")
        )));
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileOpResult { success: false, error: Some(e), .. } if e.contains("disabled")
        )));
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileDownloadError { error, .. } if error.contains("disabled")
        )));
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileUploadError { error, .. } if error.contains("disabled")
        )));
    }

    #[tokio::test]
    async fn test_dispatch_file_download_cancel_is_silent_noop() {
        // FileDownloadCancel has no capability gate and no response; it just
        // calls cancel_download. Cancelling an unknown transfer is a no-op.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"file_download_cancel","transfer_id":"nope"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.sent_count(), 0);
    }

    #[tokio::test]
    async fn test_dispatch_file_ops_success_with_enabled_manager() {
        // File manager enabled with a real temp root: exercise the success
        // branches (mkdir -> write -> list -> stat -> read -> move -> delete).
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cfg = enabled_file_cfg(&root);
        let mut h = Harness::new(ALL_CAPS, cfg);
        let mut sink = RecordingSink::new();

        let sub = root.join("sub");
        let file_a = sub.join("a.txt");
        let file_b = sub.join("b.txt");
        let mkdir = format!(
            r#"{{"type":"file_mkdir","msg_id":"mk","path":"{}"}}"#,
            sub.to_string_lossy()
        );
        h.dispatch(&mkdir, &mut sink).await.unwrap();

        // `validate_path` canonicalizes, which requires the target to already
        // exist — mirror the file_manager's own tests by pre-creating an empty
        // file so the write overwrites it.
        std::fs::write(&file_a, "").unwrap();

        // base64("hi") == "aGk="
        let write = format!(
            r#"{{"type":"file_write","msg_id":"w","path":"{}","content":"aGk="}}"#,
            file_a.to_string_lossy()
        );
        h.dispatch(&write, &mut sink).await.unwrap();

        let list = format!(
            r#"{{"type":"file_list","msg_id":"ls","path":"{}"}}"#,
            sub.to_string_lossy()
        );
        h.dispatch(&list, &mut sink).await.unwrap();

        let stat = format!(
            r#"{{"type":"file_stat","msg_id":"st","path":"{}"}}"#,
            file_a.to_string_lossy()
        );
        h.dispatch(&stat, &mut sink).await.unwrap();

        let read = format!(
            r#"{{"type":"file_read","msg_id":"rd","path":"{}","max_size":1024}}"#,
            file_a.to_string_lossy()
        );
        h.dispatch(&read, &mut sink).await.unwrap();

        let mv = format!(
            r#"{{"type":"file_move","msg_id":"mv","from":"{}","to":"{}"}}"#,
            file_a.to_string_lossy(),
            file_b.to_string_lossy()
        );
        h.dispatch(&mv, &mut sink).await.unwrap();

        let del = format!(
            r#"{{"type":"file_delete","msg_id":"del","path":"{}","recursive":false}}"#,
            file_b.to_string_lossy()
        );
        h.dispatch(&del, &mut sink).await.unwrap();

        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 7, "seven file ops, seven responses");

        // mkdir succeeded
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileOpResult { msg_id, success: true, .. } if msg_id == "mk"
        )));
        // write succeeded
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileOpResult { msg_id, success: true, .. } if msg_id == "w"
        )));
        // list returned at least the written file, no error
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileListResult { msg_id, error: None, entries, .. }
                if msg_id == "ls" && entries.iter().any(|e| e.name == "a.txt")
        )));
        // stat found the entry
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileStatResult { msg_id, entry: Some(_), error: None } if msg_id == "st"
        )));
        // read returned the base64 content of "hi"
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileReadResult { msg_id, content: Some(c), error: None }
                if msg_id == "rd" && c == "aGk="
        )));
        // move + delete succeeded
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileOpResult { msg_id, success: true, .. } if msg_id == "mv"
        )));
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileOpResult { msg_id, success: true, .. } if msg_id == "del"
        )));
    }

    #[tokio::test]
    async fn test_dispatch_file_upload_success_round_trip() {
        // Enabled manager: start -> chunk -> end upload, all on the sink.
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let cfg = enabled_file_cfg(&root);
        let mut h = Harness::new(ALL_CAPS, cfg);
        let mut sink = RecordingSink::new();

        let dest = root.join("up.bin");
        // "hi" -> base64 "aGk=" -> 2 bytes
        let start = format!(
            r#"{{"type":"file_upload_start","transfer_id":"u1","path":"{}","size":2}}"#,
            dest.to_string_lossy()
        );
        h.dispatch(&start, &mut sink).await.unwrap();
        h.dispatch(
            r#"{"type":"file_upload_chunk","transfer_id":"u1","offset":0,"data":"aGk="}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_upload_end","transfer_id":"u1"}"#,
            &mut sink,
        )
        .await
        .unwrap();

        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 3);
        // start ack at offset 0
        assert!(matches!(
            &msgs[0],
            AgentMessage::FileUploadAck { transfer_id, offset: 0 } if transfer_id == "u1"
        ));
        // chunk ack advances offset to 2
        assert!(matches!(
            &msgs[1],
            AgentMessage::FileUploadAck { transfer_id, offset: 2 } if transfer_id == "u1"
        ));
        // upload complete
        assert!(matches!(
            &msgs[2],
            AgentMessage::FileUploadComplete { transfer_id } if transfer_id == "u1"
        ));
        // bytes actually landed on disk
        assert_eq!(std::fs::read(&dest).unwrap(), b"hi");
    }

    #[tokio::test]
    async fn test_dispatch_docker_start_stats_unavailable_emits_unavailable() {
        // docker_manager is None -> DockerStartStats replies DockerUnavailable
        // and leaves the stats interval unset.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"docker_start_stats","interval_secs":2}"#,
            &mut sink,
        )
        .await
        .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(
            &msgs[0],
            AgentMessage::DockerUnavailable { msg_id: None }
        ));
        assert!(!h.runtime.docker.stats_polling_active());
    }

    #[tokio::test]
    async fn test_dispatch_docker_stop_stats_clears_interval() {
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        // Pre-seed an interval so we can observe it being cleared.
        h.runtime.docker.arm_stats_polling_for_test();
        let mut sink = RecordingSink::new();
        h.dispatch(r#"{"type":"docker_stop_stats"}"#, &mut sink)
            .await
            .unwrap();
        assert_eq!(sink.sent_count(), 0);
        assert!(!h.runtime.docker.stats_polling_active());
    }

    #[tokio::test]
    async fn test_dispatch_docker_request_unavailable_carries_msg_id() {
        // Request variants with docker_manager None reply DockerUnavailable
        // echoing the request's msg_id.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"docker_list_containers","msg_id":"req-1"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(
            &msgs[0],
            AgentMessage::DockerUnavailable { msg_id: Some(id) } if id == "req-1"
        ));

        // An event variant with no msg_id replies with msg_id: None.
        let mut sink2 = RecordingSink::new();
        h.dispatch(r#"{"type":"docker_events_start"}"#, &mut sink2)
            .await
            .unwrap();
        let msgs2 = sink2.agent_messages();
        assert_eq!(msgs2.len(), 1);
        assert!(matches!(
            &msgs2[0],
            AgentMessage::DockerUnavailable { msg_id: None }
        ));
    }

    #[tokio::test]
    async fn test_dispatch_blocklist_reset_returns_ack() {
        // FirewallManager uses the real CliNftExecutor. On a host without `nft`
        // (macOS CI) BlocklistReset deterministically fails the wipe but still
        // returns a BlocklistResetAck reply, which the dispatcher forwards.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(r#"{"type":"blocklist_reset"}"#, &mut sink)
            .await
            .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1, "reset always produces an ack");
        assert!(matches!(&msgs[0], AgentMessage::BlocklistResetAck { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_ip_quality_sync_and_run_now_respect_capability() {
        // With CAP_IP_QUALITY present, sync/run_now are accepted (no WS output).
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"ip_quality_sync","services":[],"interval_hours":12}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(r#"{"type":"ip_quality_run_now"}"#, &mut sink)
            .await
            .unwrap();
        assert_eq!(sink.sent_count(), 0);

        // Without CAP_IP_QUALITY, both are silently ignored as well.
        let caps = ALL_CAPS & !serverbee_common::constants::CAP_IP_QUALITY;
        let mut h2 = Harness::new(caps, FileConfig::default());
        let mut sink2 = RecordingSink::new();
        h2.dispatch(
            r#"{"type":"ip_quality_sync","services":[],"interval_hours":6}"#,
            &mut sink2,
        )
        .await
        .unwrap();
        h2.dispatch(r#"{"type":"ip_quality_run_now"}"#, &mut sink2)
            .await
            .unwrap();
        assert_eq!(sink2.sent_count(), 0);
    }

    #[tokio::test]
    async fn test_dispatch_blocklist_sync_add_remove_forward_acks() {
        // Sync/Add/Remove all route into FirewallManager and forward its
        // BlocklistAck reply over the WS sink. On a host without `nft` the
        // apply fails but the manager still returns an ack (with Failed state),
        // so the dispatcher always emits exactly one frame per request.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());

        // Full-state sync with one entry.
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"blocklist_sync","entries":[{"id":"e1","target":"1.2.3.4/32","family":4}]}"#,
            &mut sink,
        )
        .await
        .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1, "sync emits one ack frame");
        assert!(matches!(&msgs[0], AgentMessage::BlocklistAck { .. }));

        // Incremental add.
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"blocklist_add","entry":{"id":"e2","target":"5.6.7.8/32","family":4}}"#,
            &mut sink,
        )
        .await
        .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1, "add emits one ack frame");
        assert!(matches!(&msgs[0], AgentMessage::BlocklistAck { .. }));

        // Incremental remove of an unknown id still produces a single-item ack.
        let mut sink = RecordingSink::new();
        h.dispatch(r#"{"type":"blocklist_remove","id":"e2"}"#, &mut sink)
            .await
            .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 1, "remove emits one ack frame");
        assert!(matches!(&msgs[0], AgentMessage::BlocklistAck { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_file_ops_disabled_manager_replies_disabled_even_with_capability() {
        // CAP_FILE present but the manager is disabled (default FileConfig has
        // enabled=false). The `!file_manager.is_enabled()` half of the guard
        // must still short-circuit with the disabled error, independent of caps.
        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        assert!(
            !h.runtime.file_manager.is_enabled(),
            "default file manager is disabled"
        );
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"file_list","msg_id":"m1","path":"/tmp"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        h.dispatch(
            r#"{"type":"file_write","msg_id":"m2","path":"/tmp/x","content":"aGk="}"#,
            &mut sink,
        )
        .await
        .unwrap();
        let msgs = sink.agent_messages();
        assert_eq!(msgs.len(), 2, "each op replies once even with cap present");
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileListResult { error: Some(e), .. } if e.contains("disabled")
        )));
        assert!(msgs.iter().any(|m| matches!(
            m,
            AgentMessage::FileOpResult { success: false, error: Some(e), .. } if e.contains("disabled")
        )));
    }

    #[tokio::test]
    async fn test_dispatch_upgrade_already_running_emits_failure_on_channel() {
        // Force the global single-flight latch to "in progress", then dispatch
        // an Upgrade with the capability present. The duplicate must be rejected
        // with an UpgradeResult error on the cmd channel (not the WS sink), and
        // the latch must be left untouched (still true) for the real holder.
        UPGRADE_IN_PROGRESS.store(true, Ordering::SeqCst);
        // Ensure we always release the global latch so other tests aren't poisoned.
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                UPGRADE_IN_PROGRESS.store(false, Ordering::SeqCst);
            }
        }
        let _guard = Guard;

        let mut h = Harness::new(ALL_CAPS, FileConfig::default());
        let mut sink = RecordingSink::new();
        h.dispatch(
            r#"{"type":"upgrade","version":"9.9.9","job_id":"dup-job"}"#,
            &mut sink,
        )
        .await
        .unwrap();
        // Nothing is written to the WS sink for the duplicate case.
        assert_eq!(
            sink.sent_count(),
            0,
            "duplicate upgrade writes to channel, not sink"
        );
        let msg = tokio::time::timeout(Duration::from_secs(5), h.events.cmd_result_rx.recv())
            .await
            .expect("failure msg expected in time")
            .expect("UpgradeResult expected");
        match msg {
            AgentMessage::UpgradeResult {
                job_id,
                target_version,
                stage,
                error,
                ..
            } => {
                assert_eq!(job_id, Some("dup-job".to_string()));
                assert_eq!(target_version, "9.9.9");
                assert_eq!(stage, UpgradeStage::Downloading);
                assert!(error.contains("already running"));
            }
            other => panic!("expected UpgradeResult, got {other:?}"),
        }
    }
}
