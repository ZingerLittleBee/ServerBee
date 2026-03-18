use bollard::container::LogsOptions;
use bollard::Docker;
use futures_util::StreamExt;
use serverbee_common::docker_types::DockerLogEntry;
use serverbee_common::protocol::AgentMessage;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

const LOG_BATCH_SIZE: usize = 50;
const LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(50);

/// Spawn a background task that streams container logs and sends batched entries.
pub fn spawn_log_session(
    docker: Docker,
    session_id: String,
    container_id: String,
    tail: Option<u64>,
    follow: bool,
    agent_tx: mpsc::Sender<AgentMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) =
            run_log_stream(&docker, &session_id, &container_id, tail, follow, &agent_tx).await
        {
            tracing::warn!(
                "Log session {session_id} for container {container_id} ended with error: {e}"
            );
        }
        tracing::debug!("Log session {session_id} finished");
    })
}

async fn run_log_stream(
    docker: &Docker,
    session_id: &str,
    container_id: &str,
    tail: Option<u64>,
    follow: bool,
    agent_tx: &mpsc::Sender<AgentMessage>,
) -> anyhow::Result<()> {
    let tail_str = tail.map(|t| t.to_string()).unwrap_or_else(|| "100".into());

    let options = LogsOptions::<String> {
        follow,
        stdout: true,
        stderr: true,
        timestamps: true,
        tail: tail_str,
        ..Default::default()
    };

    let mut stream = docker.logs(container_id, Some(options));
    let mut batch: Vec<DockerLogEntry> = Vec::with_capacity(LOG_BATCH_SIZE);
    let mut last_flush = Instant::now();

    loop {
        let timeout = tokio::time::sleep(LOG_FLUSH_INTERVAL);
        tokio::pin!(timeout);

        tokio::select! {
            item = stream.next() => {
                match item {
                    Some(Ok(output)) => {
                        let (stream_name, message) = parse_log_output(&output);
                        let (timestamp, msg) = split_timestamp(&message);

                        batch.push(DockerLogEntry {
                            timestamp,
                            stream: stream_name,
                            message: msg,
                        });

                        if batch.len() >= LOG_BATCH_SIZE {
                            flush_batch(session_id, &mut batch, agent_tx).await;
                            last_flush = Instant::now();
                        }
                    }
                    Some(Err(e)) => {
                        // Flush remaining entries before returning error
                        if !batch.is_empty() {
                            flush_batch(session_id, &mut batch, agent_tx).await;
                        }
                        return Err(e.into());
                    }
                    None => {
                        // Stream ended
                        if !batch.is_empty() {
                            flush_batch(session_id, &mut batch, agent_tx).await;
                        }
                        return Ok(());
                    }
                }
            }
            _ = &mut timeout => {
                if !batch.is_empty() && last_flush.elapsed() >= LOG_FLUSH_INTERVAL {
                    flush_batch(session_id, &mut batch, agent_tx).await;
                    last_flush = Instant::now();
                }
            }
        }
    }
}

async fn flush_batch(
    session_id: &str,
    batch: &mut Vec<DockerLogEntry>,
    agent_tx: &mpsc::Sender<AgentMessage>,
) {
    let entries = std::mem::take(batch);
    let msg = AgentMessage::DockerLog {
        session_id: session_id.to_string(),
        entries,
    };
    if agent_tx.send(msg).await.is_err() {
        tracing::debug!("Log session {session_id}: agent channel closed");
    }
}

fn parse_log_output(output: &bollard::container::LogOutput) -> (String, String) {
    match output {
        bollard::container::LogOutput::StdOut { message } => {
            ("stdout".into(), String::from_utf8_lossy(message).to_string())
        }
        bollard::container::LogOutput::StdErr { message } => {
            ("stderr".into(), String::from_utf8_lossy(message).to_string())
        }
        bollard::container::LogOutput::StdIn { message } => {
            ("stdin".into(), String::from_utf8_lossy(message).to_string())
        }
        bollard::container::LogOutput::Console { message } => {
            ("console".into(), String::from_utf8_lossy(message).to_string())
        }
    }
}

/// Split a log line into (timestamp, message) if timestamps are enabled.
/// Docker log timestamps look like: "2026-03-18T10:00:00.000000000Z rest of message"
fn split_timestamp(line: &str) -> (Option<String>, String) {
    // Timestamps are typically 30+ chars like "2026-03-18T10:00:00.000000000Z "
    if line.len() > 31
        && line.as_bytes().get(4) == Some(&b'-')
        && line.as_bytes().get(10) == Some(&b'T')
        && let Some(space_pos) = line[..35.min(line.len())].find(' ')
    {
        let ts = line[..space_pos].to_string();
        let msg = line[space_pos + 1..].to_string();
        return (Some(ts), msg);
    }
    (None, line.to_string())
}
