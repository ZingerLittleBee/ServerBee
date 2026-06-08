use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Instant;

use hickory_resolver::TokioResolver;
use hickory_resolver::config::{NameServerConfig, ResolverConfig};
use hickory_resolver::proto::rr::RecordType;
use hickory_resolver::proto::xfer::Protocol;
use serde_json::{Value, json};
use serverbee_common::ssrf;

use super::CheckResult;

/// Check DNS resolution for `target` (a domain name).
///
/// Config options:
/// - `record_type`: "A", "AAAA", "CNAME", "MX", "TXT" (default "A")
/// - `expected_values`: optional JSON array of expected values
/// - `nameserver`: optional nameserver IP to query
pub async fn check(target: &str, config: &Value) -> CheckResult {
    let start = Instant::now();

    let record_type = config
        .get("record_type")
        .and_then(|v| v.as_str())
        .unwrap_or("A");

    let expected_values: Option<Vec<String>> = config.get("expected_values").and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect()
        })
    });

    let nameserver = config.get("nameserver").and_then(|v| v.as_str());

    // Build resolver configuration
    let resolver = match build_resolver(nameserver) {
        Ok(r) => r,
        Err(e) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("Failed to build DNS resolver: {e}")),
            };
        }
    };

    let ns_display = nameserver.unwrap_or("system default");

    // Resolve based on record type
    let values = match resolve_record(&resolver, target, record_type).await {
        Ok(v) => v,
        Err(e) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: json!({
                    "record_type": record_type,
                    "values": [],
                    "nameserver": ns_display,
                }),
                error: Some(format!("DNS resolution failed: {e}")),
            };
        }
    };

    let latency = start.elapsed().as_secs_f64() * 1000.0;

    // Check against expected values if provided
    let (success, changed) = if let Some(ref expected) = expected_values {
        let mut sorted_values = values.clone();
        sorted_values.sort();
        let mut sorted_expected = expected.clone();
        sorted_expected.sort();
        let matches = sorted_values == sorted_expected;
        (matches, !matches)
    } else {
        // If no expected values, success = resolution succeeded and returned results
        (!values.is_empty(), false)
    };

    let detail = json!({
        "record_type": record_type,
        "values": values,
        "nameserver": ns_display,
        "changed": changed,
    });

    let error = if !success {
        if let Some(expected) = &expected_values {
            Some(format!(
                "DNS values {values:?} did not match expected {expected:?}"
            ))
        } else {
            Some("DNS resolution returned no results".to_string())
        }
    } else {
        None
    };

    CheckResult {
        success,
        latency: Some(latency),
        detail,
        error,
    }
}

fn build_resolver(nameserver: Option<&str>) -> Result<TokioResolver, String> {
    if let Some(ns) = nameserver {
        let ip = IpAddr::from_str(ns).map_err(|e| format!("Invalid nameserver IP '{ns}': {e}"))?;
        // SSRF guard: an attacker-supplied nameserver must not point the server
        // at loopback/link-local/metadata resolvers (private resolvers are
        // allowed for internal monitoring).
        if !ssrf::is_monitor_safe_addr(ip) {
            return Err(format!(
                "nameserver '{ns}' is a blocked address (loopback/link-local/metadata)"
            ));
        }
        let ns_config = NameServerConfig::new(SocketAddr::new(ip, 53), Protocol::Udp);
        let mut resolver_config = ResolverConfig::new();
        resolver_config.add_name_server(ns_config);
        Ok(TokioResolver::builder_with_config(resolver_config, Default::default()).build())
    } else {
        TokioResolver::builder_tokio()
            .map(|builder| builder.build())
            .map_err(|e| format!("Failed to create system resolver: {e}"))
    }
}

async fn resolve_record(
    resolver: &TokioResolver,
    target: &str,
    record_type: &str,
) -> Result<Vec<String>, String> {
    match record_type.to_uppercase().as_str() {
        "A" => {
            let response = resolver
                .ipv4_lookup(target)
                .await
                .map_err(|e| e.to_string())?;
            Ok(response.iter().map(|ip| ip.to_string()).collect())
        }
        "AAAA" => {
            let response = resolver
                .ipv6_lookup(target)
                .await
                .map_err(|e| e.to_string())?;
            Ok(response.iter().map(|ip| ip.to_string()).collect())
        }
        "CNAME" => {
            let response = resolver
                .lookup(target, RecordType::CNAME)
                .await
                .map_err(|e| e.to_string())?;
            Ok(response
                .record_iter()
                .filter(|r| r.record_type() == RecordType::CNAME)
                .map(|r| r.data().to_string())
                .collect())
        }
        "MX" => {
            let response = resolver
                .mx_lookup(target)
                .await
                .map_err(|e| e.to_string())?;
            Ok(response
                .iter()
                .map(|mx| format!("{} {}", mx.preference(), mx.exchange()))
                .collect())
        }
        "TXT" => {
            let response = resolver
                .txt_lookup(target)
                .await
                .map_err(|e| e.to_string())?;
            Ok(response.iter().map(|txt| txt.to_string()).collect())
        }
        _ => Err(format!("Unsupported record type: {record_type}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_resolver_system_default() {
        let result = build_resolver(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_resolver_custom_nameserver() {
        let result = build_resolver(Some("8.8.8.8"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_resolver_invalid_nameserver() {
        let result = build_resolver(Some("not-an-ip"));
        assert!(result.is_err());
    }

    #[test]
    fn test_build_resolver_rejects_loopback_nameserver() {
        // SSRF guard: cannot point the resolver at loopback or cloud metadata.
        assert!(build_resolver(Some("127.0.0.1")).is_err());
        assert!(build_resolver(Some("169.254.169.254")).is_err());
    }

    #[test]
    fn test_build_resolver_allows_private_nameserver() {
        // Internal resolvers (RFC1918) are a legitimate monitoring setup.
        assert!(build_resolver(Some("10.0.0.53")).is_ok());
    }
}
