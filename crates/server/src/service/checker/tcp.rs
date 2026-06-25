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

    #[test]
    fn test_split_host_port_ipv6_missing_closing_bracket() {
        // `[`-prefixed but no closing `]`: the split_once('\]') fails -> None.
        assert_eq!(split_host_port("[fd00::1:443"), None);
    }

    #[test]
    fn test_split_host_port_ipv6_missing_port_colon() {
        // Bracketed host closes with `]` but the remainder lacks the `:port`.
        assert_eq!(split_host_port("[fd00::1]"), None);
    }

    #[test]
    fn test_split_host_port_ipv6_non_numeric_port() {
        // Bracketed IPv6 with a non-numeric port fails the parse -> None.
        assert_eq!(split_host_port("[fd00::1]:https"), None);
    }

    #[test]
    fn test_split_host_port_ipv6_empty_port() {
        // Bracketed IPv6 with an empty port string fails u16 parse -> None.
        assert_eq!(split_host_port("[fd00::1]:"), None);
    }

    #[test]
    fn test_split_host_port_port_out_of_range() {
        // 70000 exceeds u16::MAX, so the port parse fails -> None.
        assert_eq!(split_host_port("example.com:70000"), None);
    }

    #[test]
    fn test_split_host_port_empty_port_after_colon() {
        // Trailing colon with no port digits fails the u16 parse -> None.
        assert_eq!(split_host_port("example.com:"), None);
    }

    #[test]
    fn test_split_host_port_min_and_max_port() {
        // Boundary ports 0 and 65535 both parse successfully.
        assert_eq!(
            split_host_port("host:0"),
            Some(("host".to_string(), 0))
        );
        assert_eq!(
            split_host_port("host:65535"),
            Some(("host".to_string(), 65535))
        );
    }

    #[test]
    fn test_split_host_port_hostname_with_port() {
        // A plain hostname (not an IP) is preserved verbatim as the host.
        assert_eq!(
            split_host_port("db.internal.example.com:5432"),
            Some(("db.internal.example.com".to_string(), 5432))
        );
    }

    #[test]
    fn test_split_host_port_unbracketed_ipv6_takes_last_colon() {
        // Without brackets, rsplit_once(':') splits on the LAST colon; here the
        // trailing "443" parses as a port and the rest becomes the "host".
        assert_eq!(
            split_host_port("fd00::1:443"),
            Some(("fd00::1".to_string(), 443))
        );
    }

    #[tokio::test]
    async fn test_tcp_invalid_target_detail_reports_not_connected() {
        // The invalid-target branch sets a connected:false detail payload.
        let result = check("garbage", &json!({})).await;
        assert!(!result.success);
        assert_eq!(result.latency, None);
        assert_eq!(result.detail, json!({ "connected": false }));
    }

    #[tokio::test]
    async fn test_tcp_loopback_detail_and_no_latency() {
        // SSRF-rejected targets report no latency and a not-connected detail.
        let result = check("127.0.0.1:80", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        assert_eq!(result.latency, None);
        assert_eq!(result.detail, json!({ "connected": false }));
    }

    #[tokio::test]
    async fn test_tcp_default_timeout_used_when_config_missing() {
        // No `timeout` key -> default applies; loopback is still SSRF-rejected,
        // which proves the config-parse path tolerates an empty config object.
        let result = check("127.0.0.1:80", &json!({})).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
    }

    #[tokio::test]
    async fn test_tcp_non_numeric_timeout_falls_back_to_default() {
        // A non-integer `timeout` value is ignored (as_u64 -> None) and the
        // check still proceeds rather than panicking.
        let result = check("169.254.169.254:80", &json!({ "timeout": "soon" })).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
    }
}
