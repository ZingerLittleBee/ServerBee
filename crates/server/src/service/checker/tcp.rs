use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serverbee_common::ssrf;
use tokio::net::TcpStream;

use super::CheckResult;

/// Parse a "host:port" (or "[ipv6]:port") target into its components.
fn split_host_port(target: &str) -> Option<(String, u16)> {
    if let Some(rest) = target.strip_prefix('[') {
        // [ipv6]:port
        let (host, after) = rest.split_once(']')?;
        let port = after.strip_prefix(':')?.parse().ok()?;
        return Some((host.to_string(), port));
    }
    let (host, port) = target.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

/// Check TCP connectivity to `target` (expected format: "host:port").
///
/// Config options:
/// - `timeout`: connection timeout in seconds (default 10)
pub async fn check(target: &str, config: &Value) -> CheckResult {
    let timeout_secs = config.get("timeout").and_then(|v| v.as_u64()).unwrap_or(10);
    let timeout = Duration::from_secs(timeout_secs);

    let start = Instant::now();

    // SSRF guard: parse host:port, reject targets resolving to blocked
    // (loopback/link-local/metadata) addresses, and connect only to the
    // validated addresses so the host cannot rebind to a different IP.
    let (host, port) = match split_host_port(target) {
        Some(hp) => hp,
        None => {
            return CheckResult {
                success: false,
                latency: None,
                detail: json!({ "connected": false }),
                error: Some(format!("Invalid TCP target '{target}' (expected host:port)")),
            };
        }
    };
    let addrs = match ssrf::resolve_and_check_monitor(&host, port) {
        Ok(addrs) => addrs,
        Err(e) => {
            return CheckResult {
                success: false,
                latency: None,
                detail: json!({ "connected": false }),
                error: Some(e.to_string()),
            };
        }
    };

    match tokio::time::timeout(timeout, TcpStream::connect(&addrs[..])).await {
        Ok(Ok(_stream)) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            CheckResult {
                success: true,
                latency: Some(latency),
                detail: json!({ "connected": true }),
                error: None,
            }
        }
        Ok(Err(e)) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            CheckResult {
                success: false,
                latency: Some(latency),
                detail: json!({ "connected": false }),
                error: Some(format!("TCP connection failed: {e}")),
            }
        }
        Err(_) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            CheckResult {
                success: false,
                latency: Some(latency),
                detail: json!({ "connected": false }),
                error: Some(format!("TCP connection timed out after {timeout_secs}s")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_host_port_ipv4() {
        assert_eq!(
            split_host_port("10.0.0.5:6379"),
            Some(("10.0.0.5".to_string(), 6379))
        );
    }

    #[test]
    fn test_split_host_port_ipv6_bracketed() {
        assert_eq!(
            split_host_port("[fd00::1]:443"),
            Some(("fd00::1".to_string(), 443))
        );
    }

    #[test]
    fn test_split_host_port_invalid() {
        assert_eq!(split_host_port("no-port"), None);
        assert_eq!(split_host_port("host:notaport"), None);
    }

    #[tokio::test]
    async fn test_tcp_blocks_loopback() {
        // SSRF guard: loopback is never a valid monitoring target.
        let result = check("127.0.0.1:80", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        assert!(
            result.error.unwrap_or_default().contains("SSRF guard"),
            "loopback should be rejected by the SSRF guard"
        );
    }

    #[tokio::test]
    async fn test_tcp_blocks_cloud_metadata() {
        let result = check("169.254.169.254:80", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
    }

    #[tokio::test]
    async fn test_tcp_allows_private_target_through_guard() {
        // Private RFC1918 is allowed past the guard (internal monitoring).
        // Whether the connection itself succeeds or fails is environment-
        // dependent; the invariant is that the guard does NOT reject it.
        let result = check("10.255.255.1:1", &json!({ "timeout": 1 })).await;
        assert!(
            !result.error.unwrap_or_default().contains("SSRF guard"),
            "private targets must be allowed past the SSRF guard"
        );
    }

    #[tokio::test]
    async fn test_tcp_invalid_target() {
        let result = check("no-port-here", &json!({})).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("Invalid TCP target"));
    }
}
