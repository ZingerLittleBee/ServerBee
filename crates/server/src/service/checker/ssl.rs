use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use serde_json::{Value, json};
use serverbee_common::ssrf;
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

    let timeout_secs = config.get("timeout").and_then(|v| v.as_u64()).unwrap_or(10);
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

    // Use an explicit crypto provider: rustls 0.23 cannot auto-select one
    // when both the aws_lc_rs and ring features are compiled in (via
    // reqwest's rustls-tls + rustls' default), and ClientConfig::builder()
    // would panic the worker. ring is always available through reqwest.
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls_config = match ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
    {
        Ok(builder) => builder
            .with_root_certificates(root_store)
            .with_no_client_auth(),
        Err(e) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("TLS configuration error: {e}")),
            };
        }
    };

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

    // SSRF guard: reject hosts resolving to blocked (loopback/link-local/
    // metadata) addresses, and connect only to the validated addresses (the TLS
    // handshake still uses `server_name` for SNI / certificate validation).
    let validated_addrs = match ssrf::resolve_and_check_monitor(&host, port) {
        Ok(addrs) => addrs,
        Err(e) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(e.to_string()),
            };
        }
    };

    // Connect TCP
    let tcp_stream = match tokio::time::timeout(timeout, TcpStream::connect(&validated_addrs[..])).await
    {
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
    // `to_rfc2822()` returns a Result; serializing it directly would emit a
    // `{"Ok": "..."}` object into detail_json (which then crashes the SPA when
    // rendered). Unwrap to a plain string with a stable fallback.
    let not_before = cert
        .validity()
        .not_before
        .to_rfc2822()
        .unwrap_or_else(|_| "unknown".to_string());
    let not_after = cert
        .validity()
        .not_after
        .to_rfc2822()
        .unwrap_or_else(|_| "unknown".to_string());

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
    let default_port = config.get("port").and_then(|v| v.as_u64()).unwrap_or(443) as u16;

    // Handle IPv6 addresses in brackets: [::1]:443
    if target.starts_with('[')
        && let Some(bracket_end) = target.find(']')
    {
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

    #[tokio::test]
    async fn test_ssl_check_builds_tls_config_without_panicking() {
        // Regression: rustls 0.23 with both aws_lc_rs and ring features
        // compiled in panics inside ClientConfig::builder() because it
        // cannot pick a process default provider. The SSL checker must
        // build its TLS config with an explicit provider and return a
        // failed result for an unreachable host, never panic the worker.
        let res = check("nonexistent.invalid:443", &json!({ "timeout": 2 })).await;
        assert!(!res.success, "unreachable host should fail, not panic");
    }

    // ---- parse_host_port: IPv6 bracketed forms ----

    #[test]
    fn test_parse_host_port_ipv6_with_port() {
        // `[ipv6]:port` — host stripped of brackets, explicit port parsed.
        let config = json!({});
        let (host, port) = parse_host_port("[::1]:8443", &config);
        assert_eq!(host, "::1");
        assert_eq!(port, 8443);
    }

    #[test]
    fn test_parse_host_port_ipv6_no_port_uses_default() {
        // `[ipv6]` with no trailing port falls back to the default (443).
        let config = json!({});
        let (host, port) = parse_host_port("[2001:db8::1]", &config);
        assert_eq!(host, "2001:db8::1");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_ipv6_no_port_uses_config_override() {
        // `[ipv6]` with no port falls back to the config-provided default port.
        let config = json!({ "port": 9443 });
        let (host, port) = parse_host_port("[fe80::1]", &config);
        assert_eq!(host, "fe80::1");
        assert_eq!(port, 9443);
    }

    #[test]
    fn test_parse_host_port_ipv6_invalid_port_uses_default() {
        // `[ipv6]:<garbage>` — unparsable port segment falls back to default.
        let config = json!({});
        let (host, port) = parse_host_port("[::1]:notaport", &config);
        assert_eq!(host, "::1");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_invalid_port_falls_through_to_bare_host() {
        // A trailing colon whose suffix is not a valid u16 must not be treated
        // as a port; the whole target is taken as the host with the default port.
        let config = json!({});
        let (host, port) = parse_host_port("example.com:notaport", &config);
        assert_eq!(host, "example.com:notaport");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_port_out_of_u16_range_falls_through() {
        // 99999 overflows u16, so the colon is not treated as a port boundary.
        let config = json!({});
        let (host, port) = parse_host_port("example.com:99999", &config);
        assert_eq!(host, "example.com:99999");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_rfind_uses_last_colon() {
        // rfind picks the *last* colon, so a non-bracketed bare IPv6-looking
        // string with a trailing numeric segment splits on the final colon.
        let config = json!({});
        let (host, port) = parse_host_port("a:b:443", &config);
        assert_eq!(host, "a:b");
        assert_eq!(port, 443);
    }

    // ---- check(): deterministic early-return branches (no live network) ----

    #[tokio::test]
    async fn test_check_invalid_server_name_returns_error() {
        // An empty host is not a valid rustls ServerName, so the check returns
        // before any DNS/TCP work. This exercises the ServerName::try_from
        // failure branch deterministically without touching the network.
        let res = check("", &json!({})).await;
        assert!(!res.success, "empty host must fail");
        assert!(res.detail.is_null(), "failure detail should be null");
        assert!(res.latency.is_some(), "latency is recorded even on failure");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("Invalid server name"),
            "expected invalid server name error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_ssrf_blocks_loopback_literal() {
        // 127.0.0.1 is a valid ServerName (IP literal) but is rejected by the
        // SSRF monitor guard for loopback. The resolution of a literal IP does
        // not hit the network, so this is deterministic and offline.
        let res = check("127.0.0.1:443", &json!({ "timeout": 1 })).await;
        assert!(!res.success, "loopback literal must be blocked");
        assert!(res.detail.is_null());
        let err = res.error.expect("error message present");
        assert!(
            err.contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_ssrf_blocks_metadata_literal() {
        // The cloud metadata link-local address must be rejected by the SSRF
        // monitor guard. Literal IP, no DNS — deterministic and offline.
        let res = check("169.254.169.254:443", &json!({ "timeout": 1 })).await;
        assert!(!res.success, "metadata address must be blocked");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_ssrf_blocks_ipv6_loopback_bracketed() {
        // `[::1]` parses to host "::1" (a valid IPv6 ServerName) and is then
        // rejected by the SSRF guard as loopback. Exercises the IPv6 bracket
        // parse path feeding into the SSRF rejection branch, offline.
        let res = check("[::1]:443", &json!({ "timeout": 1 })).await;
        assert!(!res.success, "IPv6 loopback must be blocked");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_custom_thresholds_do_not_panic_on_failure() {
        // Exercises the warning_days / critical_days / timeout config parsing
        // branches with explicit values on an unreachable host, confirming the
        // checker reads them and still returns a clean failure (never panics).
        let res = check(
            "nonexistent.invalid",
            &json!({ "warning_days": 30, "critical_days": 10, "timeout": 1, "port": 443 }),
        )
        .await;
        assert!(!res.success, "unreachable host should fail");
        assert!(res.error.is_some(), "an error message should be set");
    }

    // ---- parse_host_port: additional boundary branches ----

    #[test]
    fn test_parse_host_port_explicit_port_zero() {
        // Port 0 is a valid u16 and must be honored as an explicit port, not
        // rejected or replaced by the default.
        let config = json!({});
        let (host, port) = parse_host_port("example.com:0", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 0);
    }

    #[test]
    fn test_parse_host_port_explicit_port_max_u16() {
        // 65535 is the maximum u16 and must parse as an explicit port.
        let config = json!({});
        let (host, port) = parse_host_port("example.com:65535", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 65535);
    }

    #[test]
    fn test_parse_host_port_ipv6_bracket_only_default_port() {
        // `[host]` immediately followed by the closing bracket and nothing else:
        // get(bracket_end + 2..) is None, so the default port applies.
        let config = json!({});
        let (host, port) = parse_host_port("[::1]", &config);
        assert_eq!(host, "::1");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_ipv6_bracket_zero_port() {
        // `[ipv6]:0` — explicit zero port inside the bracketed-IPv6 branch.
        let config = json!({});
        let (host, port) = parse_host_port("[2001:db8::1]:0", &config);
        assert_eq!(host, "2001:db8::1");
        assert_eq!(port, 0);
    }

    #[test]
    fn test_parse_host_port_leading_colon_empty_host() {
        // A leading colon with a numeric suffix yields an empty host and the
        // parsed port (rfind hits the only colon at index 0).
        let config = json!({});
        let (host, port) = parse_host_port(":8443", &config);
        assert_eq!(host, "");
        assert_eq!(port, 8443);
    }

    #[test]
    fn test_parse_host_port_trailing_colon_empty_suffix_uses_default() {
        // A trailing colon with an empty port suffix cannot parse to u16, so the
        // whole target (including the colon) becomes the host with default port.
        let config = json!({});
        let (host, port) = parse_host_port("example.com:", &config);
        assert_eq!(host, "example.com:");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_bare_ipv4_default_port() {
        // A bare IPv4 literal with no colon takes the whole string as host and
        // the default port.
        let config = json!({});
        let (host, port) = parse_host_port("10.0.0.5", &config);
        assert_eq!(host, "10.0.0.5");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_config_override_ignored_when_explicit() {
        // When the target carries an explicit port, the config `port` override
        // is ignored — the explicit port wins.
        let config = json!({ "port": 9999 });
        let (host, port) = parse_host_port("example.com:8443", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
    }

    #[test]
    fn test_parse_host_port_non_integer_config_port_falls_back_to_443() {
        // A non-integer `port` in config (a string) is not a u64, so as_u64()
        // returns None and the hardcoded default 443 is used.
        let config = json!({ "port": "8443" });
        let (host, port) = parse_host_port("example.com", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
    }

    // ---- check(): config-coercion + remaining early-return branches (offline) ----

    #[tokio::test]
    async fn test_check_string_typed_config_values_use_defaults() {
        // String-typed timeout/warning_days/critical_days are not u64/i64, so
        // as_u64()/as_i64() return None and defaults apply. The host is then
        // SSRF-rejected (loopback literal), proving the defaults path is taken
        // without ever touching the network for a success branch.
        let res = check(
            "127.0.0.1:443",
            &json!({ "timeout": "1", "warning_days": "30", "critical_days": "10" }),
        )
        .await;
        assert!(!res.success, "loopback literal must be blocked");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_ssrf_rejection_records_latency_and_null_detail() {
        // The SSRF rejection branch must still report a measured latency and a
        // null detail payload, mirroring every other early-return failure shape.
        let res = check("169.254.169.254", &json!({ "timeout": 1 })).await;
        assert!(!res.success);
        assert!(res.detail.is_null(), "failure detail should be null");
        assert!(
            res.latency.is_some(),
            "latency is recorded even on SSRF rejection"
        );
    }

    #[tokio::test]
    async fn test_check_invalid_server_name_with_whitespace() {
        // A host containing a space is not a valid rustls ServerName and is
        // rejected before any DNS/TCP work — deterministic, offline.
        let res = check("bad host name", &json!({ "timeout": 1 })).await;
        assert!(!res.success, "whitespace host must fail");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("Invalid server name"),
            "expected invalid server name error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_unresolvable_host_is_ssrf_rejected_offline() {
        // A `.invalid` TLD never resolves; resolve_and_check_monitor returns an
        // SSRF-guard resolution error before any TCP connect. This exercises the
        // resolution-failure rejection branch without a live network success.
        let res = check("nope.invalid", &json!({ "timeout": 1 })).await;
        assert!(!res.success, "unresolvable host must fail");
        assert!(res.error.is_some(), "an error message should be set");
        assert!(res.detail.is_null());
    }

    #[tokio::test]
    async fn test_check_ipv6_metadata_bracketed_blocked() {
        // The IPv6 link-local metadata-style literal in bracket form parses to a
        // valid ServerName and is then blocked by the SSRF guard as link-local.
        let res = check("[fe80::1]:443", &json!({ "timeout": 1 })).await;
        assert!(!res.success, "IPv6 link-local must be blocked");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    // ---- parse_host_port: malformed bracket + multi-colon edge branches ----

    #[test]
    fn test_parse_host_port_bracket_without_colon_separator_uses_default() {
        // `[host]X` where the char after `]` is not `:` : get(bracket_end + 2..)
        // points past the stray char, fails to parse a u16, and the default port
        // is used while the host is still stripped of its brackets.
        let config = json!({});
        let (host, port) = parse_host_port("[::1]x", &config);
        assert_eq!(host, "::1");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_bracket_with_trailing_garbage_after_port() {
        // `[host]:80junk` — the suffix after `]:` is not a clean u16, so parsing
        // fails and the bracketed branch falls back to the default port.
        let config = json!({});
        let (host, port) = parse_host_port("[2001:db8::1]:80junk", &config);
        assert_eq!(host, "2001:db8::1");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_unclosed_bracket_falls_through_to_rfind() {
        // A leading `[` with no closing `]` skips the bracket branch entirely and
        // is handled by the generic rfind-colon logic: the last colon's numeric
        // suffix becomes the port, the rest (including `[`) the host.
        let config = json!({});
        let (host, port) = parse_host_port("[::1:8443", &config);
        assert_eq!(host, "[::1");
        assert_eq!(port, 8443);
    }

    #[test]
    fn test_parse_host_port_unclosed_bracket_no_port_uses_default() {
        // A leading `[` with no `]` and no parseable trailing port falls all the
        // way through to the bare-host default-port return.
        let config = json!({});
        let (host, port) = parse_host_port("[hostonly", &config);
        assert_eq!(host, "[hostonly");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_empty_target_uses_default_port() {
        // An empty target has no colon and no bracket; it becomes an empty host
        // with the default port. (Mirrors the empty-host check() failure path.)
        let config = json!({});
        let (host, port) = parse_host_port("", &config);
        assert_eq!(host, "");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_null_config_port_falls_back_to_443() {
        // An explicit JSON null `port` is not a u64, so as_u64() yields None and
        // the hardcoded 443 default is used for a bare host.
        let config = json!({ "port": null });
        let (host, port) = parse_host_port("example.com", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_config_port_out_of_u16_range_truncates() {
        // A config `port` larger than u16::MAX is read as u64 then cast `as u16`,
        // which truncates (70000 & 0xFFFF == 4464). Documents the lossy cast.
        let config = json!({ "port": 70000 });
        let (host, port) = parse_host_port("example.com", &config);
        assert_eq!(host, "example.com");
        assert_eq!(port, 4464);
    }

    // ---- check(): remaining offline config-coercion branches ----

    #[tokio::test]
    async fn test_check_negative_thresholds_parse_without_panic() {
        // Negative warning_days / critical_days are valid i64 values; the config
        // parsing branch must accept them. The loopback literal is then blocked
        // by the SSRF guard, so this stays offline while proving the i64 path.
        let res = check(
            "127.0.0.1:443",
            &json!({ "warning_days": -5, "critical_days": -1, "timeout": 1 }),
        )
        .await;
        assert!(!res.success, "loopback literal must be blocked");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_config_port_routes_into_ssrf_guard() {
        // The `port` config value (not an inline target port) must feed into the
        // SSRF resolution check. A loopback literal with a config-supplied port is
        // still rejected, proving the config port flows through parse_host_port.
        let res = check("127.0.0.1", &json!({ "port": 8443, "timeout": 1 })).await;
        assert!(!res.success, "loopback literal must be blocked");
        let err = res.error.expect("error message present");
        assert!(
            err.contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_zero_timeout_still_returns_clean_failure() {
        // A zero timeout is a valid u64 and is honored; on a blocked loopback the
        // SSRF guard short-circuits before the timeout matters, yielding a clean
        // failure rather than a panic or hang.
        let res = check("127.0.0.1:443", &json!({ "timeout": 0 })).await;
        assert!(!res.success, "loopback literal must be blocked");
        assert!(res.error.is_some(), "an error message should be set");
        assert!(res.detail.is_null(), "failure detail should be null");
    }
}
