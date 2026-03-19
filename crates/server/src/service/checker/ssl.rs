use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use x509_parser::prelude::*;

use super::CheckResult;

/// Check SSL/TLS certificate for `target` (domain name, optionally with ":port").
///
/// Config options:
/// - `port`: port to connect to (default 443)
/// - `warning_days`: days before expiry to warn (default 14)
/// - `critical_days`: days before expiry to consider failure (default 7)
/// - `timeout`: connection timeout in seconds (default 10)
pub async fn check(target: &str, config: &Value) -> CheckResult {
    let start = Instant::now();

    // Parse target — may be "domain" or "domain:port"
    let (host, port) = parse_host_port(target, config);

    let timeout_secs = config
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(10);
    let warning_days = config
        .get("warning_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(14);
    let critical_days = config
        .get("critical_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(7);

    let timeout = Duration::from_secs(timeout_secs);

    // Build a rustls config that captures the peer certificate
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let tls_config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(tls_config));

    let server_name = match ServerName::try_from(host.clone()) {
        Ok(name) => name,
        Err(e) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("Invalid server name '{host}': {e}")),
            };
        }
    };

    let addr = format!("{host}:{port}");

    // Connect TCP
    let tcp_stream = match tokio::time::timeout(timeout, TcpStream::connect(&addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("TCP connection to {addr} failed: {e}")),
            };
        }
        Err(_) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("TCP connection to {addr} timed out")),
            };
        }
    };

    // TLS handshake
    let tls_stream =
        match tokio::time::timeout(timeout, connector.connect(server_name, tcp_stream)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                return CheckResult {
                    success: false,
                    latency: Some(latency),
                    detail: Value::Null,
                    error: Some(format!("TLS handshake failed: {e}")),
                };
            }
            Err(_) => {
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                return CheckResult {
                    success: false,
                    latency: Some(latency),
                    detail: Value::Null,
                    error: Some("TLS handshake timed out".to_string()),
                };
            }
        };

    let latency = start.elapsed().as_secs_f64() * 1000.0;

    // Extract peer certificates
    let (_, conn) = tls_stream.get_ref();
    let peer_certs = match conn.peer_certificates() {
        Some(certs) if !certs.is_empty() => certs,
        _ => {
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some("No peer certificates received".to_string()),
            };
        }
    };

    // Parse the leaf certificate
    let leaf_der = &peer_certs[0];
    let (_, cert) = match X509Certificate::from_der(leaf_der.as_ref()) {
        Ok(c) => c,
        Err(e) => {
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("Failed to parse X.509 certificate: {e}")),
            };
        }
    };

    let subject = cert.subject().to_string();
    let issuer = cert.issuer().to_string();
    let not_before = cert.validity().not_before.to_rfc2822();
    let not_after = cert.validity().not_after.to_rfc2822();

    // Calculate days remaining
    let now = chrono::Utc::now();
    let expiry_ts = cert.validity().not_after.timestamp();
    let days_remaining = (expiry_ts - now.timestamp()) / 86400;

    // SHA-256 fingerprint
    let fingerprint = {
        let mut hasher = Sha256::new();
        hasher.update(leaf_der.as_ref());
        let result = hasher.finalize();
        result
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    };

    let success = days_remaining > critical_days;

    let mut detail = json!({
        "subject": subject,
        "issuer": issuer,
        "not_before": not_before,
        "not_after": not_after,
        "days_remaining": days_remaining,
        "sha256_fingerprint": fingerprint,
    });

    if days_remaining <= warning_days {
        detail["warning"] = json!(format!(
            "Certificate expires in {days_remaining} days (warning threshold: {warning_days})"
        ));
    }

    CheckResult {
        success,
        latency: Some(latency),
        detail,
        error: if success {
            None
        } else {
            Some(format!(
                "Certificate expires in {days_remaining} days (critical threshold: {critical_days})"
            ))
        },
    }
}

/// Parse the host and port from target string. Supports "host" and "host:port".
fn parse_host_port(target: &str, config: &Value) -> (String, u16) {
    let default_port = config
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(443) as u16;

    // Handle IPv6 addresses in brackets: [::1]:443
    if target.starts_with('[') && let Some(bracket_end) = target.find(']') {
        let host = target[1..bracket_end].to_string();
        let port = target
            .get(bracket_end + 2..)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(default_port);
        return (host, port);
    }

    // host:port
    if let Some(colon) = target.rfind(':')
        && let Ok(port) = target[colon + 1..].parse::<u16>()
    {
        return (target[..colon].to_string(), port);
    }

    (target.to_string(), default_port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_host_port_default() {
        let config = json!({});
        let (host, port) = parse_host_port("example.com", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_explicit() {
        let config = json!({});
        let (host, port) = parse_host_port("example.com:8443", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
    }

    #[test]
    fn test_parse_host_port_config_override() {
        let config = json!({ "port": 8443 });
        let (host, port) = parse_host_port("example.com", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
    }
}
