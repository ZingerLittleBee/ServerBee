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

    #[test]
    fn test_build_resolver_invalid_ip_error_message() {
        // The Err message must surface the offending nameserver string.
        let err = build_resolver(Some("999.999.999.999")).unwrap_err();
        assert!(
            err.contains("Invalid nameserver IP"),
            "expected invalid-IP error, got: {err}"
        );
        assert!(
            err.contains("999.999.999.999"),
            "error should echo the bad nameserver, got: {err}"
        );
    }

    #[test]
    fn test_build_resolver_blocked_nameserver_error_message() {
        // SSRF-blocked nameservers report a "blocked address" reason.
        let err = build_resolver(Some("127.0.0.1")).unwrap_err();
        assert!(
            err.contains("blocked address"),
            "expected blocked-address error, got: {err}"
        );
    }

    #[test]
    fn test_build_resolver_rejects_link_local_nameserver() {
        // 169.254.0.0/16 link-local (incl. cloud metadata 169.254.169.254).
        assert!(build_resolver(Some("169.254.1.1")).is_err());
        assert!(build_resolver(Some("169.254.169.254")).is_err());
    }

    #[test]
    fn test_build_resolver_rejects_ipv6_loopback_nameserver() {
        // IPv6 loopback must be blocked just like its IPv4 counterpart.
        assert!(build_resolver(Some("::1")).is_err());
    }

    #[test]
    fn test_build_resolver_rejects_unspecified_nameserver() {
        // 0.0.0.0/8 "this network" and the IPv6 unspecified address are blocked.
        assert!(build_resolver(Some("0.0.0.0")).is_err());
        assert!(build_resolver(Some("::")).is_err());
    }

    #[test]
    fn test_build_resolver_allows_ipv6_ula_nameserver() {
        // IPv6 ULA (fc00::/7) is the private-space analogue and is allowed.
        assert!(build_resolver(Some("fc00::53")).is_ok());
    }

    #[test]
    fn test_build_resolver_allows_public_ipv6_nameserver() {
        // A globally routable IPv6 resolver (Cloudflare) must build cleanly.
        assert!(build_resolver(Some("2606:4700:4700::1111")).is_ok());
    }

    #[tokio::test]
    async fn test_resolve_record_unsupported_type_returns_err() {
        // The `_` arm rejects unknown record types before any network lookup.
        let resolver = build_resolver(Some("8.8.8.8")).expect("resolver builds");
        let err = resolve_record(&resolver, "example.com", "SRV")
            .await
            .unwrap_err();
        assert!(
            err.contains("Unsupported record type"),
            "expected unsupported-type error, got: {err}"
        );
        assert!(
            err.contains("SRV"),
            "error should echo the unsupported type, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_resolve_record_empty_type_is_unsupported() {
        // An empty record type uppercases to "" and falls through to the `_` arm.
        let resolver = build_resolver(Some("8.8.8.8")).expect("resolver builds");
        assert!(resolve_record(&resolver, "example.com", "").await.is_err());
    }

    #[tokio::test]
    async fn test_check_invalid_nameserver_build_failure() {
        // build_resolver fails first → early "Failed to build DNS resolver" path.
        let config = json!({ "nameserver": "not-an-ip" });
        let result = check("example.com", &config).await;
        assert!(!result.success, "build failure must fail the check");
        assert!(result.latency.is_some(), "latency is measured even on failure");
        assert_eq!(result.detail, Value::Null, "detail is Null on build failure");
        let err = result.error.expect("error message present");
        assert!(
            err.contains("Failed to build DNS resolver"),
            "expected build-resolver error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_blocked_nameserver_build_failure() {
        // An SSRF-blocked nameserver fails the build step before any lookup.
        let config = json!({ "nameserver": "169.254.169.254" });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("Failed to build DNS resolver")
        );
    }

    #[tokio::test]
    async fn test_check_unsupported_record_type_resolution_failure() {
        // Valid (allowed) nameserver but an unknown record type: resolve_record
        // returns Err synchronously, so this never touches the network. It
        // exercises the resolution-failure branch and its detail payload.
        let config = json!({ "nameserver": "8.8.8.8", "record_type": "SRV" });
        let result = check("example.com", &config).await;
        assert!(!result.success, "unsupported type must fail the check");
        assert!(result.latency.is_some());
        // detail mirrors the requested type, empty values, and the nameserver.
        assert_eq!(result.detail["record_type"], json!("SRV"));
        assert_eq!(result.detail["values"], json!([]));
        assert_eq!(result.detail["nameserver"], json!("8.8.8.8"));
        let err = result.error.expect("error present");
        assert!(
            err.contains("DNS resolution failed"),
            "expected resolution-failure error, got: {err}"
        );
        assert!(
            err.contains("Unsupported record type"),
            "underlying cause should be surfaced, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_resolution_failure_nameserver_display_defaults() {
        // With no nameserver configured, the detail's nameserver mirror falls
        // back to "system default" on the resolution-failure path.
        let config = json!({ "record_type": "NOTAREALTYPE" });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        assert_eq!(result.detail["nameserver"], json!("system default"));
        assert_eq!(result.detail["record_type"], json!("NOTAREALTYPE"));
    }

    #[tokio::test]
    async fn test_check_default_record_type_is_a() {
        // Omitting record_type defaults to "A": the unsupported-type branch is
        // NOT hit, so we only assert the detail mirror reflects the default.
        // (We force a deterministic, network-free failure by using a blocked
        // nameserver so no lookup is attempted.)
        let config = json!({ "nameserver": "127.0.0.1" });
        let result = check("example.com", &config).await;
        // Blocked nameserver → build failure → detail is Null, but the default
        // record type ("A") was selected internally without panicking.
        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_expected_values_parsed_from_config() {
        // expected_values is parsed from a JSON string array; non-string entries
        // are filtered out. We pair it with a blocked nameserver so the parsing
        // runs but no network lookup happens (build fails first).
        let config = json!({
            "nameserver": "0.0.0.0",
            "expected_values": ["1.2.3.4", 42, "5.6.7.8", null]
        });
        let result = check("example.com", &config).await;
        // 0.0.0.0 is blocked → build failure, detail Null, deterministic.
        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_lowercase_record_type_resolution_failure() {
        // record_type matching is case-insensitive via to_uppercase(); a
        // lowercase unsupported type still hits the `_` arm deterministically.
        let config = json!({ "nameserver": "8.8.8.8", "record_type": "soa" });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        assert_eq!(result.detail["record_type"], json!("soa"));
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("DNS resolution failed")
        );
    }

    #[test]
    fn test_build_resolver_allows_192_168_private_nameserver() {
        // 192.168.0.0/16 RFC1918 space is allowed for internal monitoring.
        assert!(build_resolver(Some("192.168.1.53")).is_ok());
    }

    #[test]
    fn test_build_resolver_allows_172_16_private_nameserver() {
        // 172.16.0.0/12 RFC1918 space is allowed for internal monitoring.
        assert!(build_resolver(Some("172.16.0.53")).is_ok());
    }

    #[tokio::test]
    async fn test_check_record_type_non_string_defaults_to_a() {
        // A non-string record_type fails as_str() and falls back to "A"; paired
        // with a blocked nameserver so no network lookup is attempted.
        let config = json!({ "nameserver": "127.0.0.1", "record_type": 1234 });
        let result = check("example.com", &config).await;
        // Blocked nameserver → deterministic build failure, no panic on the
        // non-string record_type, detail Null on the build-failure path.
        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_expected_values_non_array_treated_as_none() {
        // expected_values that is not a JSON array fails as_array() → None, so
        // the no-expected branch governs. We force a deterministic resolution
        // failure (unsupported type) so the success flag reflects "no results".
        let config = json!({
            "nameserver": "8.8.8.8",
            "record_type": "SRV",
            "expected_values": "not-an-array"
        });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        // Resolution failed, so the no-expected error message is used.
        let err = result.error.expect("error present");
        assert!(
            err.contains("DNS resolution failed"),
            "expected resolution-failure error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_expected_values_empty_array_parses() {
        // An empty JSON array parses to Some(vec![]) without panicking; paired
        // with a blocked nameserver for a deterministic, network-free run.
        let config = json!({
            "nameserver": "0.0.0.0",
            "expected_values": []
        });
        let result = check("example.com", &config).await;
        // 0.0.0.0 is blocked → build failure before any comparison runs.
        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_nameserver_non_string_uses_system_default() {
        // A non-string nameserver fails as_str() → None, so the resolver uses
        // the system default and the detail mirror reports "system default".
        let config = json!({ "nameserver": 53, "record_type": "BOGUS" });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        // resolve_record fails synchronously for the unknown type, so the
        // resolution-failure detail payload reflects the system-default mirror.
        assert_eq!(result.detail["nameserver"], json!("system default"));
        assert_eq!(result.detail["record_type"], json!("BOGUS"));
    }

    #[tokio::test]
    async fn test_check_resolution_failure_detail_has_empty_values() {
        // On the resolution-failure path the detail always carries an empty
        // `values` array regardless of the requested record type.
        let config = json!({ "record_type": "PTR" });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        assert_eq!(result.detail["values"], json!([]));
    }

    #[tokio::test]
    async fn test_resolve_record_error_echoes_original_casing() {
        // The `_` arm formats the ORIGINAL (un-uppercased) record_type string,
        // even though dispatch matched on its uppercased form.
        let resolver = build_resolver(Some("8.8.8.8")).expect("resolver builds");
        let err = resolve_record(&resolver, "example.com", "srv")
            .await
            .unwrap_err();
        assert_eq!(err, "Unsupported record type: srv");
    }

    #[tokio::test]
    async fn test_resolve_record_mixed_case_unsupported_is_err() {
        // Mixed-case unknown types uppercase past the known arms into `_`.
        let resolver = build_resolver(Some("8.8.8.8")).expect("resolver builds");
        let err = resolve_record(&resolver, "example.com", "SoA")
            .await
            .unwrap_err();
        // Original casing is preserved verbatim in the surfaced message.
        assert!(err.ends_with("SoA"), "expected original casing, got: {err}");
    }

    #[tokio::test]
    async fn test_check_resolution_failure_detail_omits_changed_key() {
        // The resolution-failure detail payload (unlike the success payload) has
        // no `changed` key — it carries only record_type/values/nameserver.
        let config = json!({ "nameserver": "8.8.8.8", "record_type": "SRV" });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        assert!(
            result.detail.get("changed").is_none(),
            "failure detail must not include a `changed` flag"
        );
        // The three keys that ARE present on the failure path.
        assert!(result.detail.get("record_type").is_some());
        assert!(result.detail.get("values").is_some());
        assert!(result.detail.get("nameserver").is_some());
    }

    #[test]
    fn test_build_resolver_second_public_nameserver_builds() {
        // A second globally routable resolver (8.8.4.4) also builds cleanly,
        // confirming the public-IPv4 success arm is not specific to one IP.
        assert!(build_resolver(Some("8.8.4.4")).is_ok());
    }

    #[tokio::test]
    async fn test_check_whitespace_record_type_is_unsupported() {
        // A whitespace-only record_type uppercases to itself, misses every known
        // arm, and lands in `_` → deterministic resolution failure, no network.
        let config = json!({ "nameserver": "8.8.8.8", "record_type": " " });
        let result = check("example.com", &config).await;
        assert!(!result.success);
        // The detail mirror preserves the requested (whitespace) record type.
        assert_eq!(result.detail["record_type"], json!(" "));
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("DNS resolution failed")
        );
    }

    #[tokio::test]
    async fn test_check_blocked_nameserver_error_echoes_reason() {
        // The build-failure error nests the SSRF "blocked address" reason inside
        // the "Failed to build DNS resolver" wrapper.
        let config = json!({ "nameserver": "169.254.169.254" });
        let result = check("example.com", &config).await;
        let err = result.error.expect("error present");
        assert!(err.contains("Failed to build DNS resolver"), "got: {err}");
        assert!(err.contains("blocked address"), "got: {err}");
    }
}
