//! Streams sshd events from `journalctl -f` (preferred) or by tailing
//! `/var/log/auth.log` / `/var/log/secure` (fallback), parses them via
//! [`crate::security::ssh_parser`], and forwards `AuthAttempt`s on `out_tx`.
//! Both paths auto-recover from transient subprocess/file failures via
//! exponential backoff.

use std::io;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use tokio::process::{Child, Command};

use crate::security::ssh_parser::{AuthAttempt, parse_sshd_line};

/// Try `journalctl` first; fall back to tailing `/var/log/auth.log` (or
/// `/var/log/secure`). On either path, retry forever with bounded
/// exponential backoff on failure.
#[cfg(target_os = "linux")]
pub async fn run_sshd_stream(out_tx: mpsc::Sender<AuthAttempt>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let result = if has_journalctl().await {
            run_journalctl_sshd(out_tx.clone()).await
        } else {
            run_auth_log_tail(out_tx.clone()).await
        };
        match result {
            Ok(()) => {
                tracing::warn!("sshd stream ended cleanly; retrying");
            }
            Err(e) => {
                tracing::warn!(error = %e, "sshd stream error; retrying after backoff");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn run_sshd_stream(_out_tx: mpsc::Sender<AuthAttempt>) {
    // No-op on non-Linux platforms.
    futures_util::future::pending::<()>().await;
}

#[cfg(target_os = "linux")]
async fn has_journalctl() -> bool {
    Command::new("journalctl")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
async fn run_journalctl_sshd(out_tx: mpsc::Sender<AuthAttempt>) -> io::Result<()> {
    let mut child = spawn_journalctl_sshd()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("journalctl stdout missing"))?;
    let reader = BufReader::new(stdout);
    let result = drain_journalctl_json(reader, out_tx).await;
    let _ = child.kill().await;
    result
}

#[cfg(target_os = "linux")]
fn spawn_journalctl_sshd() -> io::Result<Child> {
    Command::new("journalctl")
        .args([
            "-f",
            "--output=json",
            "-n",
            "0",
            // OpenSSH ≥9.8 logs auth events from the per-connection
            // "sshd-session" helper, not "sshd". Match both identifiers/comms so
            // detection keeps working on Debian 13 / Ubuntu 24.10+ / Fedora 40+,
            // including socket-activated setups where _SYSTEMD_UNIT is a
            // per-connection transient unit rather than ssh.service.
            "SYSLOG_IDENTIFIER=sshd",
            "+",
            "SYSLOG_IDENTIFIER=sshd-session",
            "+",
            "_SYSTEMD_UNIT=ssh.service",
            "+",
            "_SYSTEMD_UNIT=sshd.service",
            "+",
            "_COMM=sshd",
            "+",
            "_COMM=sshd-session",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
}

/// Read journalctl JSON lines from an arbitrary async reader, parse them,
/// and forward decoded `AuthAttempt`s on `out_tx`.
///
/// Made public for tests; the production path constructs the reader from
/// the spawned child's stdout.
pub async fn drain_journalctl_json<R: AsyncRead + Unpin>(
    reader: BufReader<R>,
    out_tx: mpsc::Sender<AuthAttempt>,
) -> io::Result<()> {
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(msg) = extract_message(&line)
            && let Some(attempt) = parse_sshd_line(&msg)
            && out_tx.send(attempt).await.is_err()
        {
            break;
        }
    }
    Ok(())
}

fn extract_message(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line).ok()?;
    let raw = v.get("MESSAGE")?;
    match raw {
        Value::String(s) => Some(s.clone()),
        // journalctl emits binary fields as JSON arrays of u8s.
        Value::Array(bytes) => {
            let mut buf = Vec::with_capacity(bytes.len());
            for b in bytes {
                buf.push(b.as_u64()? as u8);
            }
            String::from_utf8(buf).ok()
        }
        _ => None,
    }
}

/// Drain `auth.log` lines from a reader and forward attempts.
///
/// Public for tests. The production tail loop calls this against a
/// file-backed reader and re-opens on EOF / inode change.
pub async fn drain_auth_log<R: AsyncBufRead + Unpin>(
    mut reader: R,
    out_tx: mpsc::Sender<AuthAttempt>,
) -> io::Result<()> {
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        // auth.log lines look like:
        // `Mar 12 13:14:15 host sshd[123]: Failed password for ...`
        // We strip everything up to the first sshd[NNN]: prefix.
        if let Some(msg) = strip_syslog_prefix(buf.trim_end())
            && let Some(attempt) = parse_sshd_line(msg)
            && out_tx.send(attempt).await.is_err()
        {
            break;
        }
    }
    Ok(())
}

fn strip_syslog_prefix(line: &str) -> Option<&str> {
    // OpenSSH ≥9.8 logs auth lines from the per-connection "sshd-session"
    // process (`sshd-session[PID]: ...`); older releases use `sshd[PID]: ...`.
    // Match the longer name first so we don't accidentally split inside it.
    let idx = line
        .find("sshd-session[")
        .or_else(|| line.find("sshd["))?;
    let rest = &line[idx..];
    let bracket = rest.find("]: ")?;
    Some(&rest[bracket + 3..])
}

#[cfg(target_os = "linux")]
async fn run_auth_log_tail(out_tx: mpsc::Sender<AuthAttempt>) -> io::Result<()> {
    use tokio::fs::File;
    use tokio::io::AsyncSeekExt;

    let path = pick_auth_log_path();
    let path = path.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no auth log found"))?;
    let mut file = File::open(&path).await?;
    file.seek(io::SeekFrom::End(0)).await?;
    let mut last_inode = inode_of(&path).ok();
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf).await {
            Ok(0) => {
                // Detect inode change (logrotate) before sleeping.
                let cur = inode_of(&path).ok();
                if cur != last_inode {
                    tracing::info!(path = %path.display(), "auth log rotated; reopening");
                    let file = File::open(&path).await?;
                    reader = BufReader::new(file);
                    last_inode = cur;
                    continue;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Ok(_) => {
                if let Some(msg) = strip_syslog_prefix(buf.trim_end())
                    && let Some(attempt) = parse_sshd_line(msg)
                    && out_tx.send(attempt).await.is_err()
                {
                    return Ok(());
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// Stream blocked-source IPs out of the kernel log.
///
/// Looks for the common UFW / iptables / nftables blob formats and extracts
/// the `SRC=...` IP. Each blocked IP is forwarded on `out_tx` so the scan
/// detector can attach `blocked_count` to emitted events.
#[cfg(target_os = "linux")]
pub async fn run_kernel_stream(out_tx: mpsc::Sender<String>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let result = run_journalctl_kernel(out_tx.clone()).await;
        match result {
            Ok(()) => tracing::warn!("kernel firewall stream ended cleanly; retrying"),
            Err(e) => {
                tracing::warn!(error = %e, "kernel firewall stream error; retrying after backoff")
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn run_kernel_stream(_out_tx: mpsc::Sender<String>) {
    futures_util::future::pending::<()>().await;
}

#[cfg(target_os = "linux")]
async fn run_journalctl_kernel(out_tx: mpsc::Sender<String>) -> io::Result<()> {
    let mut child = spawn_journalctl_kernel()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("journalctl -k stdout missing"))?;
    let reader = BufReader::new(stdout);
    let result = drain_kernel_json(reader, out_tx).await;
    let _ = child.kill().await;
    result
}

#[cfg(target_os = "linux")]
fn spawn_journalctl_kernel() -> io::Result<Child> {
    Command::new("journalctl")
        .args(["-k", "-f", "--output=json", "-n", "0"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
}

/// Read kernel journal JSON lines and forward extracted source IPs.
///
/// Public for tests; the production path constructs the reader from
/// `journalctl -k`'s stdout.
pub async fn drain_kernel_json<R: AsyncRead + Unpin>(
    reader: BufReader<R>,
    out_tx: mpsc::Sender<String>,
) -> io::Result<()> {
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(msg) = extract_message(&line)
            && let Some(ip) = extract_blocked_ip(&msg)
            && out_tx.send(ip).await.is_err()
        {
            break;
        }
    }
    Ok(())
}

/// Extract `SRC=...` from a UFW / iptables / nftables kernel log line.
/// Returns `None` for lines that don't look like a firewall block.
fn extract_blocked_ip(msg: &str) -> Option<String> {
    if !msg.contains("[UFW BLOCK]")
        && !msg.contains("iptables")
        && !msg.contains("nftables")
        && !msg.contains("nf_log")
    {
        return None;
    }
    let idx = msg.find("SRC=")?;
    let rest = &msg[idx + 4..];
    let end = rest
        .find(|c: char| c.is_whitespace())
        .unwrap_or(rest.len());
    let ip = &rest[..end];
    if ip.is_empty() {
        None
    } else {
        Some(ip.to_string())
    }
}

#[cfg(target_os = "linux")]
fn pick_auth_log_path() -> Option<std::path::PathBuf> {
    for p in ["/var/log/auth.log", "/var/log/secure"] {
        if Path::new(p).exists() {
            return Some(p.into());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn inode_of(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(path)?.ino())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn drain_journalctl_json_emits_attempts() {
        let input = concat!(
            r#"{"MESSAGE":"Failed password for root from 1.2.3.4 port 22 ssh2","SYSLOG_IDENTIFIER":"sshd"}"#,
            "\n",
            r#"{"MESSAGE":"unrelated line","SYSLOG_IDENTIFIER":"sshd"}"#,
            "\n",
            r#"{"MESSAGE":"Accepted publickey for ubuntu from 5.6.7.8 port 1234 ssh2: ED25519 abc","SYSLOG_IDENTIFIER":"sshd"}"#,
            "\n",
        );
        let (tx, mut rx) = mpsc::channel::<AuthAttempt>(8);
        let reader = BufReader::new(input.as_bytes());
        drain_journalctl_json(reader, tx).await.unwrap();
        let a = rx.recv().await.unwrap();
        assert_eq!(a.source_ip, "1.2.3.4");
        let b = rx.recv().await.unwrap();
        assert_eq!(b.source_ip, "5.6.7.8");
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn drain_journalctl_json_handles_binary_message() {
        // journalctl emits non-UTF8 fields as a byte array.
        let bytes: Vec<u8> = b"Failed password for root from 1.2.3.4 port 22 ssh2".to_vec();
        let array_str = serde_json::to_string(&bytes).unwrap();
        let line = format!(r#"{{"MESSAGE":{array_str}}}"#);
        let (tx, mut rx) = mpsc::channel(4);
        let reader = BufReader::new(line.as_bytes());
        drain_journalctl_json(reader, tx).await.unwrap();
        let a = rx.recv().await.unwrap();
        assert_eq!(a.source_ip, "1.2.3.4");
    }

    #[tokio::test]
    async fn drain_auth_log_skips_non_sshd_lines() {
        let input = concat!(
            "Mar 12 13:14:15 host kernel: oom-killer fired\n",
            "Mar 12 13:14:16 host sshd[1234]: Failed password for root from 1.2.3.4 port 22 ssh2\n",
            "Mar 12 13:14:17 host cron[222]: (root) CMD test\n",
            "Mar 12 13:14:18 host sshd[1235]: Accepted publickey for ubuntu from 5.6.7.8 port 4242 ssh2: ED25519 abc\n",
        );
        let (tx, mut rx) = mpsc::channel(8);
        drain_auth_log(BufReader::new(input.as_bytes()), tx)
            .await
            .unwrap();
        let a = rx.recv().await.unwrap();
        assert_eq!(a.source_ip, "1.2.3.4");
        let b = rx.recv().await.unwrap();
        assert_eq!(b.source_ip, "5.6.7.8");
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn drain_kernel_json_extracts_ufw_block() {
        let input = concat!(
            r#"{"MESSAGE":"[UFW BLOCK] IN=eth0 OUT= MAC=... SRC=203.0.113.5 DST=10.0.0.1 LEN=40 ..."}"#,
            "\n",
            r#"{"MESSAGE":"unrelated kernel chatter"}"#,
            "\n",
            r#"{"MESSAGE":"iptables denied: IN=eth0 SRC=198.51.100.7 DST=10.0.0.1"}"#,
            "\n",
        );
        let (tx, mut rx) = mpsc::channel::<String>(8);
        let reader = BufReader::new(input.as_bytes());
        drain_kernel_json(reader, tx).await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), "203.0.113.5");
        assert_eq!(rx.recv().await.unwrap(), "198.51.100.7");
        assert!(rx.recv().await.is_none());
    }

    #[test]
    fn extract_blocked_ip_handles_nftables() {
        let msg = "nftables drop: IN=eth0 SRC=10.20.30.40 DST=10.0.0.1 PROTO=TCP";
        assert_eq!(extract_blocked_ip(msg).as_deref(), Some("10.20.30.40"));
    }

    #[test]
    fn extract_blocked_ip_returns_none_on_unrelated() {
        assert!(extract_blocked_ip("ATA bus error").is_none());
        assert!(extract_blocked_ip("[UFW BLOCK] missing src").is_none());
    }

    #[test]
    fn strip_syslog_prefix_works() {
        let l = "Mar 12 13:14:15 host sshd[1234]: Failed password for root from 1.2.3.4 port 22 ssh2";
        assert_eq!(
            strip_syslog_prefix(l),
            Some("Failed password for root from 1.2.3.4 port 22 ssh2")
        );
        assert_eq!(strip_syslog_prefix("Mar 12 13:14:15 host kernel: foo"), None);
    }

    #[test]
    fn strip_syslog_prefix_handles_sshd_session() {
        // OpenSSH ≥9.8 emits auth lines from the "sshd-session" helper.
        let l =
            "Jun 14 01:23:18 host sshd-session[957410]: Failed password for invalid user qa from 127.0.0.1 port 2850 ssh2";
        assert_eq!(
            strip_syslog_prefix(l),
            Some("Failed password for invalid user qa from 127.0.0.1 port 2850 ssh2")
        );
    }
}
