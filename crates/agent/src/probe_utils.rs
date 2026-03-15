use std::time::{Duration, Instant};

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
    let start = Instant::now();

    let addr = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:80")
    };

    let result =
        tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await;

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

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
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
            // Extract received count: look for ", N received,"
            if let Some(received) = parse_field_before(line, " received") {
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
    fn test_parse_ping_time() {
        assert!(
            (parse_ping_time("64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.34 ms").unwrap()
                - 12.34)
                .abs()
                < 0.001
        );
        assert!(parse_ping_time("no match here").is_none());
    }
}
