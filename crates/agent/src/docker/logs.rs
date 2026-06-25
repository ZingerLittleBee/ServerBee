use bollard::Docker;
use bollard::container::LogsOptions;
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
        bollard::container::LogOutput::StdOut { message } => (
            "stdout".into(),
            String::from_utf8_lossy(message).to_string(),
        ),
        bollard::container::LogOutput::StdErr { message } => (
            "stderr".into(),
            String::from_utf8_lossy(message).to_string(),
        ),
        bollard::container::LogOutput::StdIn { message } => {
            ("stdin".into(), String::from_utf8_lossy(message).to_string())
        }
        bollard::container::LogOutput::Console { message } => (
            "console".into(),
            String::from_utf8_lossy(message).to_string(),
        ),
    }
}

/// Split a log line into (timestamp, message) if timestamps are enabled.
/// Docker log timestamps look like: "2026-03-18T10:00:00.000000000Z rest of message"
fn split_timestamp(line: &str) -> (Option<String>, String) {
    // Timestamps are typically 30+ chars like "2026-03-18T10:00:00.000000000Z "
    // The timestamp is always ASCII, so we can safely index by bytes for the checks,
    // but must use find(' ') on the full string to avoid splitting inside multi-byte chars.
    if line.len() > 31
        && line.as_bytes().get(4) == Some(&b'-')
        && line.as_bytes().get(10) == Some(&b'T')
        && let Some(space_pos) = line.find(' ')
        && space_pos <= 35
    {
        let ts = line[..space_pos].to_string();
        let msg = line[space_pos + 1..].to_string();
        return (Some(ts), msg);
    }
    (None, line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::container::LogOutput;

    #[test]
    fn test_parse_log_output_stdout() {
        // `Bytes` is constructed via `Into` from a Vec<u8> so the `bytes`
        // crate does not need to be a direct dependency.
        let out = LogOutput::StdOut {
            message: b"hello world".to_vec().into(),
        };
        let (stream, msg) = parse_log_output(&out);
        assert_eq!(stream, "stdout", "stdout variant must map to 'stdout'");
        assert_eq!(msg, "hello world", "message bytes must decode verbatim");
    }

    #[test]
    fn test_parse_log_output_stderr() {
        let out = LogOutput::StdErr {
            message: b"oops".to_vec().into(),
        };
        let (stream, msg) = parse_log_output(&out);
        assert_eq!(stream, "stderr", "stderr variant must map to 'stderr'");
        assert_eq!(msg, "oops");
    }

    #[test]
    fn test_parse_log_output_stdin() {
        let out = LogOutput::StdIn {
            message: b"input".to_vec().into(),
        };
        let (stream, msg) = parse_log_output(&out);
        assert_eq!(stream, "stdin", "stdin variant must map to 'stdin'");
        assert_eq!(msg, "input");
    }

    #[test]
    fn test_parse_log_output_console() {
        let out = LogOutput::Console {
            message: b"console line".to_vec().into(),
        };
        let (stream, msg) = parse_log_output(&out);
        assert_eq!(stream, "console", "console variant must map to 'console'");
        assert_eq!(msg, "console line");
    }

    #[test]
    fn test_parse_log_output_invalid_utf8_is_lossy() {
        // Invalid UTF-8 bytes must be replaced, not panic.
        let out = LogOutput::StdOut {
            message: vec![0xff, 0xfe, 0x66].into(),
        };
        let (stream, msg) = parse_log_output(&out);
        assert_eq!(stream, "stdout");
        assert!(
            msg.contains('\u{FFFD}'),
            "invalid utf8 must produce the replacement char, got {msg:?}"
        );
    }

    #[test]
    fn test_split_timestamp_valid_line() {
        // Standard docker timestamp prefix is split off.
        let line = "2026-03-18T10:00:00.000000000Z rest of the message";
        let (ts, msg) = split_timestamp(line);
        assert_eq!(
            ts.as_deref(),
            Some("2026-03-18T10:00:00.000000000Z"),
            "timestamp prefix must be extracted"
        );
        assert_eq!(msg, "rest of the message", "remainder must drop the space");
    }

    #[test]
    fn test_split_timestamp_short_line_no_timestamp() {
        // Line too short (<= 31 chars) -> no timestamp.
        let line = "short message";
        let (ts, msg) = split_timestamp(line);
        assert!(ts.is_none(), "short line must not yield a timestamp");
        assert_eq!(msg, "short message", "message returned unchanged");
    }

    #[test]
    fn test_split_timestamp_wrong_format_no_dash_at_4() {
        // Long line but byte 4 is not '-' -> no timestamp.
        let line = "abcdefghijTklmnopqrstuvwxyz0123456789 trailing";
        let (ts, msg) = split_timestamp(line);
        assert!(ts.is_none(), "missing dash at index 4 must skip parsing");
        assert_eq!(msg, line, "message returned unchanged");
    }

    #[test]
    fn test_split_timestamp_wrong_format_no_t_at_10() {
        // Byte 4 is '-' but byte 10 is not 'T' -> no timestamp.
        let line = "2026-03-18 10:00:00.000000000Z some message here padded";
        let (ts, msg) = split_timestamp(line);
        assert!(ts.is_none(), "missing 'T' at index 10 must skip parsing");
        assert_eq!(msg, line, "message returned unchanged");
    }

    #[test]
    fn test_split_timestamp_no_space() {
        // Looks like a timestamp prefix but has no space separator at all.
        let line = "2026-03-18T10:00:00.000000000Z_no_space_anywhere_here_long";
        let (ts, msg) = split_timestamp(line);
        assert!(ts.is_none(), "no space means no split");
        assert_eq!(msg, line, "message returned unchanged");
    }

    #[test]
    fn test_split_timestamp_space_too_late() {
        // First space appears past index 35 -> rejected.
        let line = "2026-03-18T10:00:00.000000000ZZZZZZZZZ message after pos 35";
        let (ts, msg) = split_timestamp(line);
        assert!(ts.is_none(), "space beyond index 35 must reject the split");
        assert_eq!(msg, line, "message returned unchanged");
    }

    #[test]
    fn test_split_timestamp_multibyte_after_timestamp() {
        // Multi-byte content after the timestamp must not panic and must be
        // preserved intact.
        let line = "2026-03-18T10:00:00.000000000Z 你好世界 hello";
        let (ts, msg) = split_timestamp(line);
        assert_eq!(ts.as_deref(), Some("2026-03-18T10:00:00.000000000Z"));
        assert_eq!(msg, "你好世界 hello", "multibyte message must survive");
    }

    #[tokio::test]
    async fn test_flush_batch_sends_and_drains() {
        // flush_batch must take the batch (leaving it empty) and deliver a
        // DockerLog message carrying those entries.
        let (tx, mut rx) = mpsc::channel::<AgentMessage>(4);
        let mut batch = vec![
            DockerLogEntry {
                timestamp: Some("2026-03-18T10:00:00Z".into()),
                stream: "stdout".into(),
                message: "line1".into(),
            },
            DockerLogEntry {
                timestamp: None,
                stream: "stderr".into(),
                message: "line2".into(),
            },
        ];

        flush_batch("sess-1", &mut batch, &tx).await;

        assert!(batch.is_empty(), "batch must be drained after flush");
        let received = rx.recv().await.expect("a message must be delivered");
        match received {
            AgentMessage::DockerLog {
                session_id,
                entries,
            } => {
                assert_eq!(session_id, "sess-1", "session id must be propagated");
                assert_eq!(entries.len(), 2, "all batched entries must be sent");
                assert_eq!(entries[0].message, "line1");
                assert_eq!(entries[1].stream, "stderr");
            }
            other => panic!("expected DockerLog message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_flush_batch_closed_channel_does_not_panic() {
        // When the receiver is dropped the send fails silently (logged only).
        let (tx, rx) = mpsc::channel::<AgentMessage>(1);
        drop(rx);
        let mut batch = vec![DockerLogEntry {
            timestamp: None,
            stream: "stdout".into(),
            message: "orphan".into(),
        }];

        // Must not panic even though the channel is closed.
        flush_batch("sess-closed", &mut batch, &tx).await;
        assert!(batch.is_empty(), "batch is still drained on send failure");
    }

    #[test]
    fn test_split_timestamp_len_boundary_32_chars() {
        // A 32-char line (> 31) with a valid prefix and a space at index 30
        // is accepted; this exercises the `line.len() > 31` lower boundary.
        let line = "2026-03-18T10:00:00.000000000Z x"; // 32 bytes, space at index 30
        assert_eq!(line.len(), 32, "guard: line must be exactly 32 bytes");
        let (ts, msg) = split_timestamp(line);
        assert_eq!(
            ts.as_deref(),
            Some("2026-03-18T10:00:00.000000000Z"),
            "32-char line above the length boundary must split"
        );
        assert_eq!(msg, "x", "single-char remainder must be preserved");
    }

    #[test]
    fn test_split_timestamp_len_exactly_31_rejected() {
        // A 31-char line fails the strict `> 31` check even with a valid prefix.
        let line = "2026-03-18T10:00:00.000000000Z "; // 31 bytes incl. trailing space
        assert_eq!(line.len(), 31, "guard: line must be exactly 31 bytes");
        let (ts, msg) = split_timestamp(line);
        assert!(ts.is_none(), "length of exactly 31 must be rejected");
        assert_eq!(msg, line, "message returned unchanged");
    }

    #[test]
    fn test_split_timestamp_space_at_index_35_accepted() {
        // The first space sits exactly at index 35 -> `space_pos <= 35` accepts it.
        // The fractional part is padded so the prefix occupies indices 0..35.
        let line = "2026-03-18T10:00:00.00000000000000Z tail";
        let space_pos = line.find(' ').expect("guard: line must contain a space");
        assert_eq!(space_pos, 35, "guard: space must be at the inclusive boundary");
        let (ts, msg) = split_timestamp(line);
        assert_eq!(
            ts.as_deref(),
            Some("2026-03-18T10:00:00.00000000000000Z"),
            "space at the inclusive index-35 boundary must split"
        );
        assert_eq!(msg, "tail", "message after the boundary space is preserved");
    }

    #[test]
    fn test_split_timestamp_space_at_index_36_rejected() {
        // One char past the boundary (space at index 36) fails `space_pos <= 35`.
        let line = "2026-03-18T10:00:00.000000000000000Z tail";
        let space_pos = line.find(' ').expect("guard: line must contain a space");
        assert_eq!(space_pos, 36, "guard: space must be one past the boundary");
        let (ts, msg) = split_timestamp(line);
        assert!(ts.is_none(), "space beyond index 35 must reject the split");
        assert_eq!(msg, line, "message returned unchanged");
    }

    #[test]
    fn test_split_timestamp_empty_message_after_timestamp() {
        // A trailing space (message empty) still splits, yielding an empty message.
        // The fractional part is lengthened so the 32-byte line clears `> 31`.
        let line = "2026-03-18T10:00:00.0000000000Z "; // 32 bytes, space at index 31
        assert!(line.len() > 31, "guard: line must clear the length check");
        let (ts, msg) = split_timestamp(line);
        assert_eq!(
            ts.as_deref(),
            Some("2026-03-18T10:00:00.0000000000Z"),
            "timestamp must be extracted even with an empty remainder"
        );
        assert_eq!(msg, "", "remainder after a trailing space must be empty");
    }

    #[test]
    fn test_parse_log_output_empty_message() {
        // Empty byte payload must decode to an empty string, not panic.
        let out = LogOutput::StdOut {
            message: Vec::new().into(),
        };
        let (stream, msg) = parse_log_output(&out);
        assert_eq!(stream, "stdout", "stream name is unaffected by empty payload");
        assert_eq!(msg, "", "empty bytes decode to an empty string");
    }

    #[test]
    fn test_parse_and_split_pipeline_matches_stream_path() {
        // Mirrors the run_log_stream hot path: parse the daemon frame, then split
        // the timestamp. This covers the same composition without a live stream.
        let out = LogOutput::StdErr {
            message: b"2026-03-18T10:00:00.000000000Z boom".to_vec().into(),
        };
        let (stream_name, message) = parse_log_output(&out);
        let (timestamp, msg) = split_timestamp(&message);
        let entry = DockerLogEntry {
            timestamp,
            stream: stream_name,
            message: msg,
        };
        assert_eq!(entry.stream, "stderr", "stderr frame maps to stderr stream");
        assert_eq!(
            entry.timestamp.as_deref(),
            Some("2026-03-18T10:00:00.000000000Z"),
            "embedded timestamp is split out of the decoded frame"
        );
        assert_eq!(entry.message, "boom", "trailing payload becomes the message");
    }

    #[tokio::test]
    async fn test_flush_batch_empty_batch_sends_empty_entries() {
        // Flushing an already-empty batch still emits a DockerLog with no entries.
        let (tx, mut rx) = mpsc::channel::<AgentMessage>(1);
        let mut batch: Vec<DockerLogEntry> = Vec::new();

        flush_batch("sess-empty", &mut batch, &tx).await;

        let received = rx.recv().await.expect("a message must still be delivered");
        match received {
            AgentMessage::DockerLog {
                session_id,
                entries,
            } => {
                assert_eq!(session_id, "sess-empty", "session id is still propagated");
                assert!(entries.is_empty(), "no entries are carried for an empty batch");
            }
            other => panic!("expected DockerLog message, got {other:?}"),
        }
    }
}
