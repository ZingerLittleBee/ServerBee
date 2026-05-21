//! Conntrack new-connection stream.
//!
//! Linux ships an excellent `conntrack -E -e NEW` event mode that emits
//! one line per new flow tracked by netfilter. We consume it as a
//! subprocess and forward `(source_ip, dst_port)` to the scan detector.
//!
//! The plan originally suggested binding to `NETLINK_NETFILTER` directly,
//! but as of `netlink-packet-netfilter` 0.2 only the nflog subprotocol is
//! implemented; conntrack parsing would require hand-rolling
//! nfnetlink_cttuple decoding, which is significant complexity for a
//! feature that's already opt-in. Using the conntrack CLI keeps the
//! surface small and testable.
//!
//! Failure modes:
//! * `conntrack` binary missing or unprivileged → spawn returns an error
//!   on the first attempt, which the manager treats as "scan detection
//!   not available; keep brute-force detection running".
//! * Subprocess crash after first success → caller re-spawns with the
//!   usual exponential backoff.

use std::io;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use tokio::process::{Child, Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConntrackEvent {
    pub source_ip: String,
    pub dst_port: u16,
}

/// Open the conntrack event stream. Returns `Err` immediately if the
/// subprocess cannot be started — the manager interprets this as
/// "disable scan detection".
#[cfg(target_os = "linux")]
pub async fn start_conntrack_stream(out_tx: mpsc::Sender<ConntrackEvent>) -> io::Result<()> {
    // First attempt: must succeed to enable the watcher.
    let mut child = spawn_conntrack()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "conntrack stdout missing"))?;
    let _ = drain_conntrack_events(BufReader::new(stdout), out_tx.clone()).await;
    let _ = child.kill().await;

    // Reconnect loop with backoff. Errors are logged but do not propagate.
    let mut backoff = Duration::from_secs(1);
    loop {
        tokio::time::sleep(backoff).await;
        match spawn_conntrack() {
            Ok(mut child) => {
                if let Some(stdout) = child.stdout.take() {
                    let _ = drain_conntrack_events(BufReader::new(stdout), out_tx.clone()).await;
                }
                let _ = child.kill().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "conntrack respawn failed");
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn start_conntrack_stream(_out_tx: mpsc::Sender<ConntrackEvent>) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "conntrack stream is only supported on Linux",
    ))
}

#[cfg(target_os = "linux")]
fn spawn_conntrack() -> io::Result<Child> {
    Command::new("conntrack")
        .args(["-E", "-e", "NEW", "-o", "extended"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
}

/// Parse a line of conntrack output and produce a `ConntrackEvent` for
/// TCP SYN_SENT new flows. Returns `None` for non-TCP or non-relevant
/// events.
///
/// Example input line (extended TCP output):
///   `[NEW] tcp      6 120 SYN_SENT src=1.2.3.4 dst=10.0.0.1 sport=54321 dport=22 ...`
pub fn parse_conntrack_line(line: &str) -> Option<ConntrackEvent> {
    if !line.contains("[NEW]") {
        return None;
    }
    if !line.contains(" tcp ") && !line.starts_with("[NEW] tcp") {
        return None;
    }
    let src = extract_kv(line, "src=")?;
    let dport = extract_kv(line, "dport=")?;
    let port: u16 = dport.parse().ok()?;
    Some(ConntrackEvent {
        source_ip: src.to_string(),
        dst_port: port,
    })
}

fn extract_kv<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let end = rest
        .find(|c: char| c.is_whitespace())
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Drain lines from an arbitrary reader. Exposed for tests so we can feed
/// canned conntrack output without invoking the subprocess.
pub async fn drain_conntrack_events<R: AsyncRead + Unpin>(
    reader: BufReader<R>,
    out_tx: mpsc::Sender<ConntrackEvent>,
) -> io::Result<()> {
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(ev) = parse_conntrack_line(&line)
            && out_tx.send(ev).await.is_err()
        {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcp_syn_sent_line() {
        let line = "[NEW] tcp      6 120 SYN_SENT src=1.2.3.4 dst=10.0.0.1 sport=54321 dport=22 [UNREPLIED] src=10.0.0.1 dst=1.2.3.4 sport=22 dport=54321";
        let ev = parse_conntrack_line(line).unwrap();
        assert_eq!(ev.source_ip, "1.2.3.4");
        assert_eq!(ev.dst_port, 22);
    }

    #[test]
    fn ignores_non_new_events() {
        let line = "[UPDATE] tcp 6 432000 ESTABLISHED src=1.2.3.4 dst=10.0.0.1 sport=54321 dport=22";
        assert!(parse_conntrack_line(line).is_none());
    }

    #[test]
    fn ignores_non_tcp() {
        let line = "[NEW] udp      17 30 src=1.2.3.4 dst=10.0.0.1 sport=53 dport=53";
        assert!(parse_conntrack_line(line).is_none());
    }

    #[tokio::test]
    async fn drain_forwards_only_matching_events() {
        let input = concat!(
            "[NEW] tcp      6 120 SYN_SENT src=1.2.3.4 dst=10.0.0.1 sport=54321 dport=22\n",
            "[UPDATE] tcp   6 432000 ESTABLISHED src=1.2.3.4 dst=10.0.0.1\n",
            "[NEW] udp      17 30 src=1.2.3.4 dst=10.0.0.1 sport=53 dport=53\n",
            "[NEW] tcp      6 120 SYN_SENT src=5.6.7.8 dst=10.0.0.1 sport=11111 dport=80\n",
        );
        let (tx, mut rx) = mpsc::channel(8);
        drain_conntrack_events(BufReader::new(input.as_bytes()), tx)
            .await
            .unwrap();
        let a = rx.recv().await.unwrap();
        assert_eq!(a.source_ip, "1.2.3.4");
        assert_eq!(a.dst_port, 22);
        let b = rx.recv().await.unwrap();
        assert_eq!(b.source_ip, "5.6.7.8");
        assert_eq!(b.dst_port, 80);
        assert!(rx.recv().await.is_none());
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn start_conntrack_stream_errors_on_non_linux() {
        let (tx, _rx) = mpsc::channel(1);
        let err = start_conntrack_stream(tx).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    // Linux acceptance test — requires root and the `conntrack` binary,
    // so it's only meaningful when run manually on the target VPS.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore]
    async fn start_conntrack_stream_requires_privileges() {
        let (tx, _rx) = mpsc::channel(1);
        let _ = start_conntrack_stream(tx).await;
    }
}
