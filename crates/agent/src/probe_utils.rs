use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serverbee_common::ssrf;

/// Build a [`ProbeResult`] representing a target that the SSRF guard refused to
/// probe. Keeps the guard's message so the operator sees why it was blocked.
fn ssrf_blocked(error: String) -> ProbeResult {
    ProbeResult {
        success: false,
        latency_ms: 0.0,
        error: Some(error),
    }
}

/// Split a probe target of the form `host`, `host:port`, `[::1]`, or `[::1]:443`
/// into its host and port components, defaulting the port to `default_port`.
/// A bare IPv6 literal (multiple colons, no brackets) is treated as host-only.
fn split_host_port(target: &str, default_port: u16) -> (String, u16) {
    if let Some(rest) = target.strip_prefix('[') {
        if let Some((host, port)) = rest.split_once("]:") {
            return (host.to_string(), port.parse().unwrap_or(default_port));
        }
        if let Some(host) = rest.strip_suffix(']') {
            return (host.to_string(), default_port);
        }
    }
    if let Some((host, port)) = target.rsplit_once(':')
        && !host.contains(':')
        && let Ok(parsed) = port.parse::<u16>()
    {
        return (host.to_string(), parsed);
    }
    (target.to_string(), default_port)
}

/// SSRF guard for server-pushed probe targets. Resolves `host` and rejects
/// loopback / link-local (incl. the 169.254.169.254 cloud-metadata endpoint) /
/// NAT64 / broadcast / this-network, while ALLOWING RFC1918 private ranges so
/// operators can legitimately probe internal hosts (same policy the
/// service-monitor checkers use). Returns the validated addresses so callers
/// connect to them directly and close the DNS-rebinding window.
fn guard_probe_target(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    ssrf::resolve_and_check_monitor(host, port).map_err(|e| e.to_string())
}

/// Result of a single probe attempt.
pub struct ProbeResult {
    pub success: bool,
    pub latency_ms: f64,
    pub error: Option<String>,
}

/// Aggregated result of a batch ICMP probe (multiple pings).
pub struct BatchIcmpResult {
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: u32,
    pub packet_received: u32,
}

/// Perform a single ICMP ping to `host` using the system `ping` command.
pub async fn probe_icmp(host: &str, timeout: Duration) -> ProbeResult {
    // SSRF guard: refuse loopback/link-local/metadata targets even for ICMP.
    if let Err(e) = guard_probe_target(host, 0) {
        return ssrf_blocked(e);
    }

    let start = Instant::now();

    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("ping")
            .args(["-c", "1", "-W", "5", host])
            .output(),
    )
    .await;

    match output {
        Ok(Ok(out)) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let latency = parse_ping_time(&stdout).unwrap_or(elapsed);
                ProbeResult {
                    success: true,
                    latency_ms: latency,
                    error: None,
                }
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                ProbeResult {
                    success: false,
                    latency_ms: 0.0,
                    error: Some(format!("Ping failed: {}", stderr.trim())),
                }
            }
        }
        Ok(Err(e)) => ProbeResult {
            success: false,
            latency_ms: 0.0,
            error: Some(format!("Failed to run ping: {e}")),
        },
        Err(_) => ProbeResult {
            success: false,
            latency_ms: 0.0,
            error: Some("Ping timed out".to_string()),
        },
    }
}

/// Perform a TCP connect probe to `host` (format `host:port`).
/// If no port is specified, port 80 is used.
pub async fn probe_tcp(host: &str, timeout: Duration) -> ProbeResult {
    let (h, port) = split_host_port(host, 80);
    // SSRF guard: validate the resolved addresses before connecting, then
    // connect to exactly those addresses (no re-resolve) so the validated and
    // connected addresses are identical.
    let addrs = match guard_probe_target(&h, port) {
        Ok(addrs) => addrs,
        Err(e) => return ssrf_blocked(e),
    };

    probe_tcp_addrs(&addrs, timeout).await
}

/// Connect to one of `addrs` (already SSRF-validated) and report reachability.
/// Split out so tests can exercise the connect logic directly against a
/// validated loopback address without tripping the guard.
pub(crate) async fn probe_tcp_addrs(addrs: &[SocketAddr], timeout: Duration) -> ProbeResult {
    let start = Instant::now();

    let result = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(addrs)).await;

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(Ok(_stream)) => ProbeResult {
            success: true,
            latency_ms: elapsed,
            error: None,
        },
        Ok(Err(e)) => ProbeResult {
            success: false,
            latency_ms: 0.0,
            error: Some(format!("TCP connect failed: {e}")),
        },
        Err(_) => ProbeResult {
            success: false,
            latency_ms: 0.0,
            error: Some("TCP connect timed out".to_string()),
        },
    }
}

/// Perform an HTTP GET probe to `url`.
pub async fn probe_http(url: &str, timeout: Duration) -> ProbeResult {
    let start = Instant::now();

    let normalized_url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    };

    // SSRF guard: validate scheme/credentials, then resolve the host and reject
    // loopback/link-local/metadata. RFC1918 stays allowed for internal monitoring.
    let parsed = match ssrf::validate_monitor_url(&normalized_url) {
        Ok(u) => u,
        Err(e) => return ssrf_blocked(e.to_string()),
    };
    let host = match parsed.host_str() {
        Some(h) => h.to_string(),
        None => return ssrf_blocked("SSRF guard: URL has no host".to_string()),
    };
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs = match guard_probe_target(&host, port) {
        Ok(addrs) => addrs,
        Err(e) => return ssrf_blocked(e),
    };

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        // Do NOT follow redirects: a 302 into 169.254.169.254 would otherwise
        // bypass the address guard. A redirection status still counts as
        // "reachable" below.
        .redirect(reqwest::redirect::Policy::none())
        // Pin the host to the addresses we just validated so reqwest does not
        // re-resolve (closes the DNS-rebinding window between check and connect).
        .resolve_to_addrs(&host, &addrs)
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return ProbeResult {
                success: false,
                latency_ms: 0.0,
                error: Some(format!("Failed to create HTTP client: {e}")),
            };
        }
    };

    match client.get(&normalized_url).send().await {
        Ok(resp) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let status = resp.status();
            if status.is_success() || status.is_redirection() {
                ProbeResult {
                    success: true,
                    latency_ms: elapsed,
                    error: None,
                }
            } else {
                ProbeResult {
                    success: false,
                    latency_ms: elapsed,
                    error: Some(format!("HTTP {status}")),
                }
            }
        }
        Err(e) => ProbeResult {
            success: false,
            latency_ms: 0.0,
            error: Some(format!("HTTP request failed: {e}")),
        },
    }
}

/// Run `ping -c N` once and parse the summary statistics.
pub async fn probe_icmp_batch(host: &str, count: u32, timeout: Duration) -> BatchIcmpResult {
    // SSRF guard: refuse loopback/link-local/metadata targets.
    if guard_probe_target(host, 0).is_err() {
        return BatchIcmpResult {
            avg_latency: None,
            min_latency: None,
            max_latency: None,
            packet_loss: 1.0,
            packet_sent: count,
            packet_received: 0,
        };
    }

    let count_str = count.to_string();
    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("ping")
            .args(["-c", &count_str, "-W", "5", host])
            .output(),
    )
    .await;

    match output {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_ping_batch_output(&stdout, count)
        }
        _ => BatchIcmpResult {
            avg_latency: None,
            min_latency: None,
            max_latency: None,
            packet_loss: 1.0,
            packet_sent: count,
            packet_received: 0,
        },
    }
}

/// Parse the output of a `ping -c N` invocation into a [`BatchIcmpResult`].
///
/// This is public so callers (and tests) can parse pre-captured output directly.
pub fn parse_ping_batch_output(output: &str, packet_sent: u32) -> BatchIcmpResult {
    let mut packet_received: u32 = 0;
    let mut packet_loss: f64 = 1.0;
    let mut min_latency: Option<f64> = None;
    let mut avg_latency: Option<f64> = None;
    let mut max_latency: Option<f64> = None;

    for line in output.lines() {
        // Parse "X packets transmitted, Y received, Z% packet loss, ..."
        if line.contains("packet loss") {
            // Extract received count: look for "N received" or "N packets received" (macOS)
            if let Some(received) = parse_field_before(line, " packets received")
                .or_else(|| parse_field_before(line, " received"))
            {
                packet_received = received.parse::<u32>().unwrap_or(0);
            }
            // Extract packet loss percentage: look for "N% packet loss"
            if let Some(loss_str) = parse_field_before(line, "% packet loss")
                && let Ok(pct) = loss_str.parse::<f64>()
            {
                packet_loss = pct / 100.0;
            }
        }

        // Parse "rtt min/avg/max/mdev = A/B/C/D ms"
        // Also handles "round-trip min/avg/max/stddev = A/B/C/D ms" (macOS)
        if line.contains("min/avg/max")
            && let Some(eq_pos) = line.find('=')
        {
            let values_part = line[eq_pos + 1..].trim();
            // Remove trailing " ms" if present
            let values_str = values_part.trim_end_matches(" ms").trim();
            let parts: Vec<&str> = values_str.splitn(4, '/').collect();
            if parts.len() >= 3 {
                min_latency = parts[0].trim().parse::<f64>().ok();
                avg_latency = parts[1].trim().parse::<f64>().ok();
                max_latency = parts[2].trim().parse::<f64>().ok();
            }
        }
    }

    // If no packets were received, clear latency fields
    if packet_received == 0 {
        min_latency = None;
        avg_latency = None;
        max_latency = None;
    }

    BatchIcmpResult {
        avg_latency,
        min_latency,
        max_latency,
        packet_loss,
        packet_sent,
        packet_received,
    }
}

/// Parse "time=X.XX ms" (or "time=X.XX") from a single ping reply line.
pub fn parse_ping_time(output: &str) -> Option<f64> {
    for line in output.lines() {
        if let Some(pos) = line.find("time=") {
            let rest = &line[pos + 5..];
            let num_str: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(ms) = num_str.parse::<f64>() {
                return Some(ms);
            }
        }
    }
    None
}

/// Helper: given a line like `"10 packets transmitted, 9 received, 10% packet loss"`,
/// extract the token immediately before `suffix` (trimmed, last whitespace-separated word).
fn parse_field_before<'a>(line: &'a str, suffix: &str) -> Option<&'a str> {
    let pos = line.find(suffix)?;
    let before = line[..pos].trim();
    // The token we want is the last whitespace-separated word before the suffix
    let token = before.split_whitespace().next_back()?;
    // Strip leading comma if present
    Some(token.trim_start_matches(','))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_batch_output_success() {
        let output = "PING 1.1.1.1 (1.1.1.1): 56 data bytes\n\n--- 1.1.1.1 ping statistics ---\n10 packets transmitted, 9 received, 10% packet loss, time 9013ms\nrtt min/avg/max/mdev = 1.234/5.678/9.012/2.345 ms";
        let result = parse_ping_batch_output(output, 10);
        assert_eq!(result.packet_sent, 10);
        assert_eq!(result.packet_received, 9);
        assert!((result.packet_loss - 0.1).abs() < 0.01);
        assert!((result.min_latency.unwrap() - 1.234).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 5.678).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 9.012).abs() < 0.001);
    }

    #[test]
    fn test_parse_batch_output_total_loss() {
        let output = "PING 192.0.2.1 (192.0.2.1): 56 data bytes\n\n--- 192.0.2.1 ping statistics ---\n10 packets transmitted, 0 received, 100% packet loss, time 9999ms";
        let result = parse_ping_batch_output(output, 10);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < 0.01);
        assert!(result.avg_latency.is_none());
    }

    #[test]
    fn test_parse_batch_output_macos_format() {
        let output = "PING 1.1.1.1 (1.1.1.1): 56 data bytes\n64 bytes from 1.1.1.1: icmp_seq=0 ttl=53 time=5.123 ms\n\n--- 1.1.1.1 ping statistics ---\n3 packets transmitted, 3 packets received, 0.0% packet loss\nround-trip min/avg/max/stddev = 4.123/5.456/6.789/1.234 ms";
        let result = parse_ping_batch_output(output, 3);
        assert_eq!(result.packet_sent, 3);
        assert_eq!(result.packet_received, 3);
        assert!((result.packet_loss - 0.0).abs() < 0.01);
        assert!((result.min_latency.unwrap() - 4.123).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 5.456).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 6.789).abs() < 0.001);
    }

    #[test]
    fn test_parse_ping_time() {
        assert!(
            (parse_ping_time("64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.34 ms").unwrap()
                - 12.34)
                .abs()
                < 0.001
        );
        assert!(parse_ping_time("no match here").is_none());
    }

    // ── SSRF guard on server-pushed probe targets ────────────────────────────
    // Probe targets are free-form strings set on the server and pushed to the
    // agent. A compromised server / rogue admin must not be able to turn the
    // agent into an internal-network / cloud-metadata scanner. We reuse the
    // monitor-grade guard: RFC1918 private ranges stay allowed (legitimate
    // internal monitoring) but loopback / link-local (incl. 169.254.169.254
    // cloud metadata) / NAT64 are blocked.

    #[tokio::test]
    async fn probe_tcp_blocks_loopback_with_ssrf_guard() {
        let r = probe_tcp("127.0.0.1:1", std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "loopback probe must not succeed");
        assert!(
            r.error.unwrap_or_default().contains("SSRF guard"),
            "loopback target must be rejected by the SSRF guard"
        );
    }

    #[tokio::test]
    async fn probe_tcp_blocks_cloud_metadata_ip() {
        let r = probe_tcp("169.254.169.254:80", std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "cloud-metadata probe must not succeed");
        assert!(
            r.error.unwrap_or_default().contains("SSRF guard"),
            "169.254.169.254 (cloud metadata) must be rejected by the SSRF guard"
        );
    }

    #[tokio::test]
    async fn probe_tcp_allows_private_rfc1918_target() {
        // Guardrail for the GREEN impl: monitor policy ALLOWS RFC1918 so internal
        // monitoring keeps working. The connect itself fails/times out, but it
        // must NOT be blocked by the SSRF guard.
        let r = probe_tcp("10.255.255.255:1", std::time::Duration::from_millis(300)).await;
        assert!(
            !r.error.unwrap_or_default().contains("SSRF guard"),
            "RFC1918 private targets must remain probeable for internal monitoring"
        );
    }

    #[tokio::test]
    async fn probe_http_blocks_loopback_with_ssrf_guard() {
        let r = probe_http("http://127.0.0.1:1", std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "loopback HTTP probe must not succeed");
        assert!(
            r.error.unwrap_or_default().contains("SSRF guard"),
            "loopback HTTP target must be rejected by the SSRF guard"
        );
    }

    #[tokio::test]
    async fn probe_http_blocks_cloud_metadata() {
        let r = probe_http(
            "http://169.254.169.254/latest/meta-data/",
            std::time::Duration::from_millis(500),
        )
        .await;
        assert!(!r.success, "cloud-metadata HTTP probe must not succeed");
        assert!(
            r.error.unwrap_or_default().contains("SSRF guard"),
            "169.254.169.254 HTTP target must be rejected by the SSRF guard"
        );
    }

    #[tokio::test]
    async fn probe_icmp_blocks_loopback_with_ssrf_guard() {
        let r = probe_icmp("127.0.0.1", std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "loopback ICMP probe must not succeed");
        assert!(
            r.error.unwrap_or_default().contains("SSRF guard"),
            "loopback ICMP target must be rejected by the SSRF guard"
        );
    }

    // ── split_host_port: every parsing branch ────────────────────────────────

    #[test]
    fn split_host_port_bracketed_ipv6_with_port() {
        // `[::1]:443` — bracketed IPv6 + explicit port via the "]:" branch.
        let (host, port) = split_host_port("[::1]:443", 80);
        assert_eq!(host, "::1");
        assert_eq!(port, 443);
    }

    #[test]
    fn split_host_port_bracketed_ipv6_no_port() {
        // `[2606:4700::1111]` — bracketed IPv6, no port: falls to default.
        let (host, port) = split_host_port("[2606:4700::1111]", 80);
        assert_eq!(host, "2606:4700::1111");
        assert_eq!(port, 80);
    }

    #[test]
    fn split_host_port_bracketed_ipv6_bad_port_falls_to_default() {
        // `[::1]:notaport` — bad port in the "]:" branch: `parse().unwrap_or`.
        let (host, port) = split_host_port("[::1]:notaport", 80);
        assert_eq!(host, "::1");
        assert_eq!(port, 80);
    }

    #[test]
    fn split_host_port_host_with_port() {
        // `example.com:8080` — plain host:port via the rsplit branch.
        let (host, port) = split_host_port("example.com:8080", 80);
        assert_eq!(host, "example.com");
        assert_eq!(port, 8080);
    }

    #[test]
    fn split_host_port_bare_ipv6_treated_as_host_only() {
        // Bare IPv6 literal (multiple colons, no brackets): the rsplit branch's
        // `!host.contains(':')` guard fails, so the whole thing is host-only.
        let (host, port) = split_host_port("::1", 80);
        assert_eq!(host, "::1");
        assert_eq!(port, 80);
    }

    #[test]
    fn split_host_port_host_with_non_numeric_port_falls_through() {
        // `example.com:notaport` — rsplit finds a colon but the port doesn't
        // parse, so the `if let Ok(parsed)` guard fails and we fall through to
        // returning the whole target as host with the default port.
        let (host, port) = split_host_port("example.com:notaport", 80);
        assert_eq!(host, "example.com:notaport");
        assert_eq!(port, 80);
    }

    #[test]
    fn split_host_port_plain_host_uses_default_port() {
        // No colon at all: rsplit_once returns None, fall through to default.
        let (host, port) = split_host_port("example.com", 443);
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn split_host_port_ipv4_with_port() {
        let (host, port) = split_host_port("192.168.0.1:5432", 80);
        assert_eq!(host, "192.168.0.1");
        assert_eq!(port, 5432);
    }

    // ── parse_field_before: direct unit coverage ─────────────────────────────

    #[test]
    fn parse_field_before_returns_token_before_suffix() {
        let line = "10 packets transmitted, 9 received, 10% packet loss";
        assert_eq!(parse_field_before(line, " received"), Some("9"));
        assert_eq!(parse_field_before(line, "% packet loss"), Some("10"));
    }

    #[test]
    fn parse_field_before_missing_suffix_returns_none() {
        assert_eq!(parse_field_before("no suffix here", " received"), None);
    }

    #[test]
    fn parse_field_before_strips_leading_comma() {
        // Last whitespace-separated token starts with a comma: the leading
        // comma must be stripped so the numeric value is isolated.
        let line = "10 transmitted ,9 received";
        assert_eq!(parse_field_before(line, " received"), Some("9"));
    }

    // ── parse_ping_time: additional branches ─────────────────────────────────

    #[test]
    fn parse_ping_time_without_unit_suffix() {
        // "time=X" with no trailing " ms" — chars are taken until a non
        // digit/dot, which here is end-of-line.
        assert!((parse_ping_time("reply time=7.5").unwrap() - 7.5).abs() < 0.001);
    }

    #[test]
    fn parse_ping_time_integer_value() {
        assert!((parse_ping_time("foo time=42 ms").unwrap() - 42.0).abs() < 0.001);
    }

    #[test]
    fn parse_ping_time_marker_but_no_number_returns_none() {
        // "time=" present but immediately followed by a non-numeric char: the
        // collected string is empty and parse fails, so the loop continues and
        // ultimately returns None.
        assert!(parse_ping_time("time=abc").is_none());
    }

    #[test]
    fn parse_ping_time_empty_input_returns_none() {
        assert!(parse_ping_time("").is_none());
    }

    #[test]
    fn parse_ping_time_scans_multiple_lines() {
        // First line has no marker; second line carries the value.
        let out = "PING start\n64 bytes: icmp_seq=1 time=3.14 ms";
        assert!((parse_ping_time(out).unwrap() - 3.14).abs() < 0.001);
    }

    // ── parse_ping_batch_output: remaining branches ──────────────────────────

    #[test]
    fn parse_batch_output_no_stats_lines() {
        // Output with neither a "packet loss" line nor a "min/avg/max" line:
        // defaults stay (loss 1.0, received 0, all latencies None).
        let result = parse_ping_batch_output("PING 1.1.1.1: 56 data bytes\n", 4);
        assert_eq!(result.packet_sent, 4);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
        assert!(result.min_latency.is_none());
        assert!(result.avg_latency.is_none());
        assert!(result.max_latency.is_none());
    }

    #[test]
    fn parse_batch_output_macos_packets_received_phrase() {
        // macOS "N packets received" must be preferred over the " received"
        // fallback (parse_field_before is tried with the longer suffix first).
        let output = "5 packets transmitted, 5 packets received, 0.0% packet loss\nround-trip min/avg/max/stddev = 1.0/2.0/3.0/0.5 ms";
        let result = parse_ping_batch_output(output, 5);
        assert_eq!(result.packet_received, 5);
        assert!((result.min_latency.unwrap() - 1.0).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 2.0).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 3.0).abs() < 0.001);
    }

    #[test]
    fn parse_batch_output_latency_line_without_equals_sign() {
        // A "min/avg/max" line lacking '=' must not panic and must leave the
        // latency fields untouched (None after the zero-received clear).
        let output = "1 packets transmitted, 0 received, 100% packet loss\nrtt min/avg/max/mdev stats unavailable";
        let result = parse_ping_batch_output(output, 1);
        assert_eq!(result.packet_received, 0);
        assert!(result.avg_latency.is_none());
    }

    #[test]
    fn parse_batch_output_latency_with_fewer_than_three_parts() {
        // Only two values after '=' — the `parts.len() >= 3` guard fails, so the
        // latencies stay None even though packets were received.
        let output = "2 packets transmitted, 2 received, 0% packet loss\nrtt min/avg/max/mdev = 1.0/2.0 ms";
        let result = parse_ping_batch_output(output, 2);
        assert_eq!(result.packet_received, 2);
        assert!(result.min_latency.is_none());
        assert!(result.avg_latency.is_none());
        assert!(result.max_latency.is_none());
    }

    #[test]
    fn parse_batch_output_clears_latency_when_zero_received() {
        // Latency line present and parseable, but 0 packets received: the final
        // guard clears all three latency fields back to None.
        let output = "3 packets transmitted, 0 received, 100% packet loss\nrtt min/avg/max/mdev = 1.0/2.0/3.0/0.5 ms";
        let result = parse_ping_batch_output(output, 3);
        assert_eq!(result.packet_received, 0);
        assert!(result.min_latency.is_none());
        assert!(result.avg_latency.is_none());
        assert!(result.max_latency.is_none());
    }

    #[test]
    fn parse_batch_output_partial_loss_percentage() {
        let output = "10 packets transmitted, 7 received, 30% packet loss, time 100ms\nrtt min/avg/max/mdev = 1.0/2.0/3.0/0.5 ms";
        let result = parse_ping_batch_output(output, 10);
        assert_eq!(result.packet_received, 7);
        assert!((result.packet_loss - 0.3).abs() < 0.01);
    }

    #[test]
    fn parse_batch_output_unparseable_loss_percentage_keeps_default() {
        // "X% packet loss" where X is not a number: the `parse::<f64>()` guard
        // fails and packet_loss keeps its 1.0 default. Received still parses.
        let output = "4 packets transmitted, 4 received, bad% packet loss\nrtt min/avg/max/mdev = 1.0/2.0/3.0/0.5 ms";
        let result = parse_ping_batch_output(output, 4);
        assert_eq!(result.packet_received, 4);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
    }

    // ── ssrf_blocked / guard_probe_target: direct helpers ────────────────────

    #[test]
    fn ssrf_blocked_builds_failed_result() {
        let r = ssrf_blocked("SSRF guard: blocked".to_string());
        assert!(!r.success);
        assert!((r.latency_ms - 0.0).abs() < f64::EPSILON);
        assert_eq!(r.error.as_deref(), Some("SSRF guard: blocked"));
    }

    #[test]
    fn guard_probe_target_rejects_loopback() {
        let err = guard_probe_target("127.0.0.1", 80).unwrap_err();
        assert!(err.contains("SSRF guard"), "got: {err}");
    }

    #[test]
    fn guard_probe_target_allows_private_rfc1918() {
        // Monitor policy keeps RFC1918 reachable for internal monitoring.
        assert!(guard_probe_target("10.0.0.5", 80).is_ok());
    }

    // ── probe_tcp: additional guard-rejection branches ───────────────────────

    #[tokio::test]
    async fn probe_tcp_blocks_bracketed_ipv6_loopback() {
        // `[::1]:443` exercises the bracketed split path + IPv6 loopback guard.
        let r = probe_tcp("[::1]:443", std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "IPv6 loopback probe must not succeed");
        assert!(
            r.error.unwrap_or_default().contains("SSRF guard"),
            "[::1] must be rejected by the SSRF guard"
        );
    }

    #[tokio::test]
    async fn probe_tcp_blocks_host_only_loopback_default_port() {
        // No port: split_host_port returns the default (80) and the guard still
        // rejects the loopback host.
        let r = probe_tcp("127.0.0.1", std::time::Duration::from_millis(500)).await;
        assert!(!r.success);
        assert!(r.error.unwrap_or_default().contains("SSRF guard"));
    }

    // ── probe_tcp_addrs: connect failure path against a closed loopback port ──

    #[tokio::test]
    async fn probe_tcp_addrs_reports_connect_failure() {
        // Connect to a validated loopback address whose port is closed. This is
        // a local connect (no outbound traffic) and fails fast with a refused
        // connection, exercising the `Ok(Err(_))` arm.
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let r = probe_tcp_addrs(&[addr], std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "closed port must not report success");
        let err = r.error.unwrap_or_default();
        assert!(
            err.contains("TCP connect"),
            "expected a TCP connect failure/timeout, got: {err}"
        );
    }

    #[tokio::test]
    async fn probe_tcp_addrs_times_out_on_empty_addr_list() {
        // An empty address slice cannot connect; tokio's connect resolves to an
        // error immediately, hitting the `Ok(Err(_))` arm.
        let r = probe_tcp_addrs(&[], std::time::Duration::from_millis(200)).await;
        assert!(!r.success);
        assert!(r.error.is_some());
    }

    // ── probe_http: normalization + guard-rejection branches ─────────────────

    #[tokio::test]
    async fn probe_http_blocks_credentials_in_url() {
        // Embedded credentials are rejected by validate_monitor_url before any
        // network access.
        let r = probe_http(
            "http://user:pass@example.com/",
            std::time::Duration::from_millis(500),
        )
        .await;
        assert!(!r.success, "credentialed URL must not succeed");
        let err = r.error.unwrap_or_default();
        assert!(
            err.contains("credentials"),
            "expected embedded-credentials rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn probe_http_normalizes_scheme_less_target_then_blocks_loopback() {
        // A bare "host[:port]" with no scheme is normalized to http:// and then
        // the loopback host is rejected by the guard. Exercises the
        // `format!("http://{url}")` normalization branch.
        let r = probe_http("127.0.0.1:1", std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "normalized loopback target must not succeed");
        assert!(
            r.error.unwrap_or_default().contains("SSRF guard"),
            "normalized loopback must be rejected by the SSRF guard"
        );
    }

    #[tokio::test]
    async fn probe_http_rejects_unparseable_url() {
        // "http://" has no host: validate_monitor_url fails to parse and the
        // error is surfaced via ssrf_blocked before any connect.
        let r = probe_http("http://", std::time::Duration::from_millis(500)).await;
        assert!(!r.success, "host-less URL must not succeed");
        assert!(r.error.is_some());
    }

    #[tokio::test]
    async fn probe_http_https_loopback_blocked() {
        // An explicit https:// loopback URL is kept as-is (not re-normalized)
        // and rejected by the address guard.
        let r = probe_http(
            "https://127.0.0.1/",
            std::time::Duration::from_millis(500),
        )
        .await;
        assert!(!r.success);
        assert!(r.error.unwrap_or_default().contains("SSRF guard"));
    }

    // ── probe_icmp_batch: guard rejection returns total-loss result ───────────

    #[tokio::test]
    async fn probe_icmp_batch_blocks_loopback_returns_total_loss() {
        let r = probe_icmp_batch("127.0.0.1", 5, std::time::Duration::from_millis(500)).await;
        assert_eq!(r.packet_sent, 5);
        assert_eq!(r.packet_received, 0);
        assert!((r.packet_loss - 1.0).abs() < f64::EPSILON);
        assert!(r.avg_latency.is_none());
        assert!(r.min_latency.is_none());
        assert!(r.max_latency.is_none());
    }

    #[tokio::test]
    async fn probe_icmp_batch_blocks_cloud_metadata() {
        let r = probe_icmp_batch(
            "169.254.169.254",
            3,
            std::time::Duration::from_millis(500),
        )
        .await;
        assert_eq!(r.packet_sent, 3);
        assert_eq!(r.packet_received, 0);
        assert!((r.packet_loss - 1.0).abs() < f64::EPSILON);
    }

    // ── parse_ping_batch_output: remaining edge branches ─────────────────────

    #[test]
    fn parse_batch_output_latency_line_without_ms_suffix() {
        // Latency line whose values have no trailing " ms": `trim_end_matches`
        // is a no-op and the slash-split still yields the three values.
        let output =
            "3 packets transmitted, 3 received, 0% packet loss\nrtt min/avg/max/mdev = 1.5/2.5/3.5/0.7";
        let result = parse_ping_batch_output(output, 3);
        assert_eq!(result.packet_received, 3);
        assert!((result.min_latency.unwrap() - 1.5).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 2.5).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 3.5).abs() < 0.001);
    }

    #[test]
    fn parse_batch_output_exactly_three_values_no_mdev() {
        // Only three slash-separated values (no mdev/stddev): `parts.len() >= 3`
        // holds and all three latencies parse; the missing 4th is ignored.
        let output =
            "2 packets transmitted, 2 received, 0% packet loss\nrtt min/avg/max = 4.0/5.0/6.0 ms";
        let result = parse_ping_batch_output(output, 2);
        assert_eq!(result.packet_received, 2);
        assert!((result.min_latency.unwrap() - 4.0).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 5.0).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 6.0).abs() < 0.001);
    }

    #[test]
    fn parse_batch_output_fractional_loss_percentage() {
        // A non-integer loss percentage like "33.3%" must parse as f64 and be
        // divided by 100 (0.333), not truncated to an integer.
        let output = "9 packets transmitted, 6 received, 33.3% packet loss, time 80ms\nrtt min/avg/max/mdev = 1.0/2.0/3.0/0.5 ms";
        let result = parse_ping_batch_output(output, 9);
        assert_eq!(result.packet_received, 6);
        assert!((result.packet_loss - 0.333).abs() < 0.0001);
    }

    #[test]
    fn parse_batch_output_unparseable_latency_values_stay_none() {
        // The three slash-separated tokens are non-numeric: each `parse::<f64>()`
        // returns None, so the latency fields stay None even with packets received.
        let output =
            "1 packets transmitted, 1 received, 0% packet loss\nrtt min/avg/max/mdev = a/b/c/d ms";
        let result = parse_ping_batch_output(output, 1);
        assert_eq!(result.packet_received, 1);
        assert!(result.min_latency.is_none());
        assert!(result.avg_latency.is_none());
        assert!(result.max_latency.is_none());
    }

    #[test]
    fn parse_batch_output_extra_slash_values_truncated_to_four() {
        // More than four slash-separated values: `splitn(4, '/')` keeps the first
        // three intact (the 4th captures the remainder); min/avg/max still parse.
        let output = "4 packets transmitted, 4 received, 0% packet loss\nrtt min/avg/max/mdev = 1.1/2.2/3.3/0.4/extra ms";
        let result = parse_ping_batch_output(output, 4);
        assert_eq!(result.packet_received, 4);
        assert!((result.min_latency.unwrap() - 1.1).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 2.2).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 3.3).abs() < 0.001);
    }

    #[test]
    fn parse_batch_output_loss_line_without_received_token() {
        // A "packet loss" line that never contains a "received" phrase: the
        // received parse finds no match and packet_received stays 0, which in
        // turn clears latency back to None.
        let output =
            "summary: 100% packet loss observed\nrtt min/avg/max/mdev = 1.0/2.0/3.0/0.5 ms";
        let result = parse_ping_batch_output(output, 6);
        assert_eq!(result.packet_sent, 6);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
        assert!(result.avg_latency.is_none());
    }

    // ── parse_ping_time: leading-dot and trailing-garbage branches ───────────

    #[test]
    fn parse_ping_time_leading_dot_value() {
        // "time=.5" — the take_while collects ".5"; f64 parses a leading-dot
        // float successfully.
        assert!((parse_ping_time("reply time=.5 ms").unwrap() - 0.5).abs() < 0.001);
    }

    #[test]
    fn parse_ping_time_stops_at_non_numeric_tail() {
        // "time=8.0ms" (no space before unit): take_while stops at 'm', leaving
        // "8.0" which parses cleanly.
        assert!((parse_ping_time("64 bytes time=8.0ms").unwrap() - 8.0).abs() < 0.001);
    }

    // ── parse_field_before: suffix at start yields empty before-token ────────

    #[test]
    fn parse_field_before_suffix_at_line_start_returns_none() {
        // The suffix occurs at position 0, so `before` is empty and
        // `split_whitespace().next_back()` returns None.
        assert_eq!(parse_field_before(" received here", " received"), None);
    }

    #[test]
    fn parse_field_before_picks_last_token_only() {
        // Multiple tokens precede the suffix; only the final whitespace-separated
        // token ("12") is returned, not the earlier words.
        let line = "transmitted and finally 12 received";
        assert_eq!(parse_field_before(line, " received"), Some("12"));
    }

    // ── split_host_port: empty-port edge after the "]:" / colon split ────────

    #[test]
    fn split_host_port_bracketed_empty_port_falls_to_default() {
        // `[::1]:` — the "]:" branch matches with an empty port string, which
        // fails to parse, so `unwrap_or(default_port)` applies.
        let (host, port) = split_host_port("[::1]:", 8080);
        assert_eq!(host, "::1");
        assert_eq!(port, 8080);
    }

    #[test]
    fn split_host_port_unterminated_bracket_treated_as_host() {
        // `[::1` — leading '[' but no closing "]:" or "]" suffix: neither inner
        // branch matches, and the bare-IPv6 guard also fails, so the whole
        // string (including the bracket) is returned as the host.
        let (host, port) = split_host_port("[::1", 80);
        assert_eq!(host, "[::1");
        assert_eq!(port, 80);
    }

    #[test]
    fn split_host_port_bracket_prefix_falls_through_to_rsplit() {
        // `[host]name:8080` — starts with '[' so the bracketed path is entered,
        // but the inner "]:" and "]" suffix branches both fail (the ']' is not at
        // the end and is not immediately followed by ':'). Execution then reaches
        // the rsplit branch: the host part has no inner colon and the port parses,
        // so the (now bracket-prefixed) host and the parsed port are returned.
        let (host, port) = split_host_port("[host]name:8080", 80);
        assert_eq!(host, "[host]name");
        assert_eq!(port, 8080);
    }

    #[test]
    fn split_host_port_empty_target_uses_default_port() {
        // Empty target: no '[' prefix, no colon — every conditional branch is
        // skipped and the empty host is returned verbatim with the default port.
        let (host, port) = split_host_port("", 80);
        assert_eq!(host, "");
        assert_eq!(port, 80);
    }

    #[test]
    fn split_host_port_port_above_u16_range_falls_to_default() {
        // `host:70000` — the colon is found but 70000 overflows u16, so the
        // `parse::<u16>()` guard in the rsplit branch fails and we fall through to
        // returning the whole target as the host with the default port.
        let (host, port) = split_host_port("host:70000", 80);
        assert_eq!(host, "host:70000");
        assert_eq!(port, 80);
    }

    // ── parse_ping_time: failed-parse line then a valid later line ────────────

    #[test]
    fn parse_ping_time_skips_garbage_marker_then_parses_later_line() {
        // The first line carries a "time=" marker whose value is non-numeric, so
        // its parse fails and the loop must CONTINUE scanning. A later line has a
        // valid "time=" value, which is the one returned. This is distinct from
        // the "first line lacks the marker entirely" case.
        let out = "first time=xyz ms\nsecond time=9.99 ms";
        assert!((parse_ping_time(out).unwrap() - 9.99).abs() < 0.001);
    }

    #[test]
    fn parse_ping_time_returns_first_valid_marker() {
        // Two valid "time=" markers on separate lines: the first one wins because
        // the function returns eagerly on the first successful parse.
        let out = "reply time=1.0 ms\nreply time=2.0 ms";
        assert!((parse_ping_time(out).unwrap() - 1.0).abs() < 0.001);
    }

    // ── parse_ping_batch_output: non-numeric received token → unwrap_or(0) ─────

    #[test]
    fn parse_batch_output_non_numeric_received_defaults_to_zero() {
        // A "packet loss" line whose token before " received" is non-numeric:
        // parse_field_before("... packets received") finds no adjacent
        // "packets received" phrase (the words are not adjacent here), so the
        // " received" fallback matches and yields "foo"; `parse::<u32>()` then
        // fails and `unwrap_or(0)` leaves packet_received at 0.
        let output = "5 packets transmitted, foo received, 50% packet loss\nrtt min/avg/max/mdev = 1.0/2.0/3.0/0.5 ms";
        let result = parse_ping_batch_output(output, 5);
        assert_eq!(result.packet_received, 0);
        // 0 received also clears latency back to None.
        assert!(result.avg_latency.is_none());
        // The loss percentage still parses independently of the received token.
        assert!((result.packet_loss - 0.5).abs() < 0.01);
    }

    // ── probe_tcp_addrs: success arm against a real local loopback listener ────

    #[tokio::test]
    async fn probe_tcp_addrs_reports_success_against_open_local_port() {
        // Bind a TCP listener on an ephemeral loopback port, then connect to its
        // real address. This is a purely local connect (no external network, no
        // privileges, no daemon) and exercises the `Ok(Ok(_stream))` success arm
        // that the guard-rejection paths never reach.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Keep accepting in the background so the connect completes cleanly.
        let accept = tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let r = probe_tcp_addrs(&[addr], std::time::Duration::from_millis(500)).await;
        assert!(r.success, "open local port must report success");
        assert!(r.error.is_none(), "successful connect must carry no error");
        assert!(r.latency_ms >= 0.0, "latency must be a non-negative duration");

        accept.abort();
    }
}
