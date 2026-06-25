use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};
use serverbee_common::ssrf;

use super::CheckResult;

/// Check an HTTP endpoint for status code and keyword presence.
///
/// Config options:
/// - `method`: "GET" or "POST" (default "GET")
/// - `keyword`: string to search for in the response body
/// - `keyword_exists`: whether the keyword should exist (default true)
/// - `expected_status`: array of acceptable status codes (default [200])
/// - `headers`: object of custom request headers
/// - `body`: optional request body string (for POST)
/// - `timeout`: request timeout in seconds (default 10)
pub async fn check(target: &str, config: &Value) -> CheckResult {
    let start = Instant::now();

    let method = config
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_uppercase();

    let keyword = config.get("keyword").and_then(|v| v.as_str());
    let keyword_exists = config
        .get("keyword_exists")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let expected_status: Vec<u16> = config
        .get("expected_status")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_u64().map(|n| n as u16))
                .collect()
        })
        .unwrap_or_else(|| vec![200]);

    let timeout_secs = config.get("timeout").and_then(|v| v.as_u64()).unwrap_or(10);

    let body_str = config.get("body").and_then(|v| v.as_str());

    // Build custom headers
    let custom_headers = match build_headers(config) {
        Ok(h) => h,
        Err(e) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("Invalid headers: {e}")),
            };
        }
    };

    // SSRF guard with per-hop revalidation. `.resolve_to_addrs` only pins the
    // original host, and reqwest's default policy would auto-follow up to 10
    // redirects — so a monitored endpoint could 3xx-redirect to
    // 169.254.169.254 or an internal host and slip past the guard entirely.
    // Disable auto-redirect and follow manually, validating the URL (scheme +
    // credentials; any port allowed for non-standard service ports) and pinning
    // the client to the freshly validated addresses on every hop. Mirrors the
    // agent's ip_quality fetcher.
    const MAX_REDIRECTS: usize = 10;

    // Early-return helper for validation/build errors (Value::Null detail).
    let fail = move |msg: String| CheckResult {
        success: false,
        latency: Some(start.elapsed().as_secs_f64() * 1000.0),
        detail: Value::Null,
        error: Some(msg),
    };

    let mut current_url = target.to_string();
    let mut hop = 0usize;

    let (status_code, response_body) = loop {
        let url = match ssrf::validate_monitor_url(&current_url) {
            Ok(u) => u,
            Err(e) => return fail(e.to_string()),
        };
        let host = match url.host_str() {
            Some(h) => h.to_string(),
            None => return fail("URL has no host".to_string()),
        };
        let port = url.port_or_known_default().unwrap_or(80);
        let validated_addrs = match ssrf::resolve_and_check_monitor(&host, port) {
            Ok(addrs) => addrs,
            Err(e) => return fail(e.to_string()),
        };

        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .danger_accept_invalid_certs(false)
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&host, &validated_addrs)
            .build()
        {
            Ok(c) => c,
            Err(e) => return fail(format!("Failed to build HTTP client: {e}")),
        };

        // The configured method/body/custom headers apply to the first hop
        // only; redirects are followed as GET without the body or custom
        // headers, so a POST body and any secrets in those headers are never
        // replayed to a redirected (possibly attacker-controlled) host.
        let request = if hop == 0 {
            let base = match method.as_str() {
                "GET" => client.get(url.clone()),
                "POST" => {
                    let mut req = client.post(url.clone());
                    if let Some(body) = body_str {
                        req = req.body(body.to_string());
                    }
                    req
                }
                other => return fail(format!("Unsupported HTTP method: {other}")),
            };
            base.headers(custom_headers.clone())
        } else {
            client.get(url.clone())
        };

        // Execute the request
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                return CheckResult {
                    success: false,
                    latency: Some(latency),
                    detail: json!({
                        "status_code": null,
                        "keyword_found": null,
                        "response_time_ms": latency,
                    }),
                    error: Some(format!("HTTP request failed: {e}")),
                };
            }
        };

        let status = response.status();

        // Follow redirects manually so every hop is SSRF-validated.
        if status.is_redirection() {
            if hop >= MAX_REDIRECTS {
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                return CheckResult {
                    success: false,
                    latency: Some(latency),
                    detail: json!({
                        "status_code": status.as_u16(),
                        "keyword_found": null,
                        "response_time_ms": latency,
                    }),
                    error: Some(format!("Too many redirects (max {MAX_REDIRECTS})")),
                };
            }
            let location = match response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            {
                Some(loc) => loc.to_string(),
                None => return fail("Redirect response had no Location header".to_string()),
            };
            // Resolve the redirect target relative to the current URL; the next
            // loop iteration re-validates and re-pins it.
            let next = match url.join(&location) {
                Ok(u) => u,
                Err(e) => return fail(format!("Invalid redirect Location: {e}")),
            };
            current_url = next.to_string();
            hop += 1;
            continue;
        }

        let status_code = status.as_u16();

        // Read response body
        let response_body = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                return CheckResult {
                    success: false,
                    latency: Some(latency),
                    detail: json!({
                        "status_code": status_code,
                        "keyword_found": null,
                        "response_time_ms": latency,
                    }),
                    error: Some(format!("Failed to read response body: {e}")),
                };
            }
        };

        break (status_code, response_body);
    };

    let latency = start.elapsed().as_secs_f64() * 1000.0;

    // Check status code
    let status_ok = expected_status.contains(&status_code);

    // Check keyword
    let keyword_found = keyword.map(|kw| response_body.contains(kw));
    let keyword_ok = match (keyword, keyword_found) {
        (Some(_), Some(found)) => found == keyword_exists,
        (None, _) => true, // No keyword check configured
        _ => true,
    };

    let success = status_ok && keyword_ok;

    let detail = json!({
        "status_code": status_code,
        "keyword_found": keyword_found,
        "response_time_ms": latency,
    });

    let error = if !success {
        let mut reasons = Vec::new();
        if !status_ok {
            reasons.push(format!(
                "Status code {status_code} not in expected {expected_status:?}"
            ));
        }
        if !keyword_ok {
            if keyword_exists {
                reasons.push(format!(
                    "Keyword '{}' not found in response",
                    keyword.unwrap_or("")
                ));
            } else {
                reasons.push(format!(
                    "Keyword '{}' found in response but should be absent",
                    keyword.unwrap_or("")
                ));
            }
        }
        Some(reasons.join("; "))
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

/// Build a `HeaderMap` from the config's `headers` object.
fn build_headers(config: &Value) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();

    if let Some(obj) = config.get("headers").and_then(|v| v.as_object()) {
        for (key, value) in obj {
            let header_name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|e| format!("Invalid header name '{key}': {e}"))?;
            let header_value = value
                .as_str()
                .ok_or_else(|| format!("Header value for '{key}' must be a string"))?;
            let header_value = HeaderValue::from_str(header_value)
                .map_err(|e| format!("Invalid header value for '{key}': {e}"))?;
            headers.insert(header_name, header_value);
        }
    }

    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_headers_empty() {
        let config = json!({});
        let headers = build_headers(&config).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn test_build_headers_with_values() {
        let config = json!({
            "headers": {
                "X-Custom-Header": "test-value",
                "Accept": "application/json"
            }
        });
        let headers = build_headers(&config).unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers.get("x-custom-header").unwrap(), "test-value");
        assert_eq!(headers.get("accept").unwrap(), "application/json");
    }

    #[test]
    fn test_build_headers_invalid_value() {
        let config = json!({
            "headers": {
                "X-Header": 123
            }
        });
        let result = build_headers(&config);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("must be a string"),
            "non-string header value should report a string-type error"
        );
    }

    #[test]
    fn test_build_headers_invalid_name() {
        // A header name containing a space is not a valid HTTP token, so
        // `HeaderName::from_bytes` rejects it.
        let config = json!({
            "headers": {
                "Invalid Header Name": "value"
            }
        });
        let result = build_headers(&config);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("Invalid header name"),
            "invalid header name should report a name error"
        );
    }

    #[test]
    fn test_build_headers_invalid_header_value_chars() {
        // A control character (newline) is not allowed in a header value.
        let config = json!({
            "headers": {
                "X-Header": "bad\nvalue"
            }
        });
        let result = build_headers(&config);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("Invalid header value"),
            "control chars in a header value should report a value error"
        );
    }

    #[test]
    fn test_build_headers_no_headers_key() {
        // `headers` absent entirely yields an empty map (the `if let` branch
        // is skipped).
        let config = json!({ "method": "GET", "timeout": 5 });
        let headers = build_headers(&config).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn test_build_headers_non_object_headers() {
        // `headers` present but not an object: `.as_object()` returns None and
        // the loop body never runs, so we get an empty map.
        let config = json!({ "headers": "not-an-object" });
        let headers = build_headers(&config).unwrap();
        assert!(headers.is_empty());
    }

    #[tokio::test]
    async fn test_check_invalid_headers_short_circuits() {
        // An invalid header type fails before any SSRF/network work, returning
        // an "Invalid headers" error with a measured (non-network) latency.
        let config = json!({
            "headers": { "X-Bad": 42 }
        });
        let result = check("https://example.com", &config).await;
        assert!(!result.success);
        let err = result.error.unwrap_or_default();
        assert!(
            err.contains("Invalid headers"),
            "expected an invalid-headers error, got: {err}"
        );
        // Detail is Null for the early build-error path.
        assert_eq!(result.detail, Value::Null);
        assert!(result.latency.is_some());
    }

    #[tokio::test]
    async fn test_check_disallowed_scheme() {
        // A non-http(s) scheme is rejected by `validate_monitor_url` before any
        // DNS resolution, so this is fully deterministic offline.
        let result = check("ftp://example.com/resource", &json!({})).await;
        assert!(!result.success);
        let err = result.error.unwrap_or_default();
        assert!(
            err.contains("scheme") && err.contains("not allowed"),
            "expected a scheme-not-allowed SSRF error, got: {err}"
        );
        // Validation failures use the `fail` helper which sets a Null detail.
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_embedded_credentials_rejected() {
        // Embedded credentials are rejected during URL validation (no DNS).
        let result = check("http://user:pass@example.com/", &json!({})).await;
        assert!(!result.success);
        let err = result.error.unwrap_or_default();
        assert!(
            err.contains("embedded credentials"),
            "expected an embedded-credentials error, got: {err}"
        );
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_unparseable_url() {
        // A string that `Url::parse` cannot parse fails in validation before
        // any network access.
        let result = check("not a url at all", &json!({})).await;
        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_blocks_loopback() {
        // A loopback literal IP resolves locally (no DNS) and is rejected by the
        // monitor SSRF guard.
        let result = check("http://127.0.0.1/health", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        let err = result.error.unwrap_or_default();
        assert!(
            err.contains("SSRF guard"),
            "loopback should be blocked by the SSRF guard, got: {err}"
        );
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_blocks_cloud_metadata() {
        // The cloud metadata endpoint (169.254.169.254) is link-local and must
        // be blocked.
        let result = check("http://169.254.169.254/latest/meta-data", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
        assert_eq!(result.detail, Value::Null);
    }

    #[test]
    fn test_expected_status_parsing_default() {
        // When `expected_status` is absent, the default of [200] applies. This
        // mirrors the parsing branch in `check`.
        let config = json!({});
        let expected_status: Vec<u16> = config
            .get("expected_status")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_u64().map(|n| n as u16))
                    .collect()
            })
            .unwrap_or_else(|| vec![200]);
        assert_eq!(expected_status, vec![200]);
    }

    #[test]
    fn test_expected_status_parsing_custom() {
        // Explicit codes are parsed; non-numeric array entries are skipped.
        let config = json!({ "expected_status": [200, 204, "skip-me", 301] });
        let expected_status: Vec<u16> = config
            .get("expected_status")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_u64().map(|n| n as u16))
                    .collect()
            })
            .unwrap_or_else(|| vec![200]);
        assert_eq!(expected_status, vec![200, 204, 301]);
    }

    #[test]
    fn test_keyword_classification_present_expected() {
        // Replicates the keyword classification logic in `check`: keyword found
        // and expected to exist => ok.
        let body = "all systems operational";
        let keyword = Some("operational");
        let keyword_exists = true;
        let keyword_found = keyword.map(|kw| body.contains(kw));
        let keyword_ok = match (keyword, keyword_found) {
            (Some(_), Some(found)) => found == keyword_exists,
            (None, _) => true,
            _ => true,
        };
        assert_eq!(keyword_found, Some(true));
        assert!(keyword_ok);
    }

    #[test]
    fn test_keyword_classification_absent_expected_present() {
        // Keyword expected but not found => not ok.
        let body = "page not found";
        let keyword = Some("operational");
        let keyword_exists = true;
        let keyword_found = keyword.map(|kw| body.contains(kw));
        let keyword_ok = match (keyword, keyword_found) {
            (Some(_), Some(found)) => found == keyword_exists,
            (None, _) => true,
            _ => true,
        };
        assert_eq!(keyword_found, Some(false));
        assert!(!keyword_ok);
    }

    #[test]
    fn test_keyword_classification_present_but_should_be_absent() {
        // keyword_exists=false: presence of the keyword fails the check.
        let body = "ERROR: database down";
        let keyword = Some("ERROR");
        let keyword_exists = false;
        let keyword_found = keyword.map(|kw| body.contains(kw));
        let keyword_ok = match (keyword, keyword_found) {
            (Some(_), Some(found)) => found == keyword_exists,
            (None, _) => true,
            _ => true,
        };
        assert_eq!(keyword_found, Some(true));
        assert!(!keyword_ok, "keyword present while expected absent must fail");
    }

    #[test]
    fn test_keyword_classification_no_keyword() {
        // No keyword configured => keyword check always passes.
        let body = "anything";
        let keyword: Option<&str> = None;
        let keyword_exists = true;
        let keyword_found = keyword.map(|kw| body.contains(kw));
        let keyword_ok = match (keyword, keyword_found) {
            (Some(_), Some(found)) => found == keyword_exists,
            (None, _) => true,
            _ => true,
        };
        assert_eq!(keyword_found, None);
        assert!(keyword_ok);
    }

    #[test]
    fn test_error_message_status_only_failure() {
        // Replicates the error-message assembly: status mismatch only.
        let status_code: u16 = 500;
        let expected_status: Vec<u16> = vec![200];
        let status_ok = expected_status.contains(&status_code);
        let keyword_ok = true;
        let success = status_ok && keyword_ok;
        assert!(!success);

        let mut reasons = Vec::new();
        if !status_ok {
            reasons.push(format!(
                "Status code {status_code} not in expected {expected_status:?}"
            ));
        }
        let msg = reasons.join("; ");
        assert!(msg.contains("Status code 500"));
        assert!(msg.contains("expected [200]"));
    }

    #[test]
    fn test_error_message_keyword_missing_failure() {
        // keyword_exists=true but keyword not found => "not found" message.
        let keyword = Some("healthy");
        let keyword_exists = true;
        let keyword_ok = false;
        let mut reasons = Vec::new();
        if !keyword_ok {
            if keyword_exists {
                reasons.push(format!(
                    "Keyword '{}' not found in response",
                    keyword.unwrap_or("")
                ));
            } else {
                reasons.push(format!(
                    "Keyword '{}' found in response but should be absent",
                    keyword.unwrap_or("")
                ));
            }
        }
        let msg = reasons.join("; ");
        assert!(msg.contains("'healthy' not found"));
    }

    #[test]
    fn test_error_message_keyword_should_be_absent_failure() {
        // keyword_exists=false and keyword present => "should be absent" message.
        let keyword = Some("maintenance");
        let keyword_exists = false;
        let keyword_ok = false;
        let mut reasons = Vec::new();
        if !keyword_ok {
            if keyword_exists {
                reasons.push(format!(
                    "Keyword '{}' not found in response",
                    keyword.unwrap_or("")
                ));
            } else {
                reasons.push(format!(
                    "Keyword '{}' found in response but should be absent",
                    keyword.unwrap_or("")
                ));
            }
        }
        let msg = reasons.join("; ");
        assert!(msg.contains("'maintenance' found in response but should be absent"));
    }

    #[test]
    fn test_method_default_and_normalization() {
        // Method parsing: absent => "GET"; provided lowercase => uppercased.
        let cfg_default = json!({});
        let method = cfg_default
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        assert_eq!(method, "GET");

        let cfg_post = json!({ "method": "post" });
        let method = cfg_post
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        assert_eq!(method, "POST");
    }

    #[test]
    fn test_keyword_exists_default_true() {
        // `keyword_exists` defaults to true when absent or non-bool.
        let cfg = json!({ "keyword": "ok" });
        let keyword_exists = cfg
            .get("keyword_exists")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        assert!(keyword_exists);

        let cfg_explicit = json!({ "keyword": "ok", "keyword_exists": false });
        let keyword_exists = cfg_explicit
            .get("keyword_exists")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        assert!(!keyword_exists);
    }

    // ── check(): additional deterministic (offline) error paths ───────────────

    // NOTE: `check()` passes `url.host_str()` (which keeps the brackets, e.g.
    // "[::1]") straight to the resolver, so bracketed IPv6 literals are rejected
    // at the resolve step (they fail `to_socket_addrs`) rather than reaching the
    // is-monitor-safe SSRF block. Either way the request is refused before any
    // HTTP call. These tests pin that contract: a bracketed IPv6 literal always
    // yields a failed result with a null detail and a non-empty error.

    #[tokio::test]
    async fn test_check_rejects_ipv6_loopback_literal() {
        let result = check("http://[::1]/health", &json!({ "timeout": 2 })).await;
        assert!(!result.success, "IPv6 loopback literal must not succeed");
        assert_eq!(result.detail, Value::Null);
        assert!(!result.error.unwrap_or_default().is_empty());
    }

    #[tokio::test]
    async fn test_check_rejects_ipv6_link_local_literal() {
        let result = check("http://[fe80::1]/", &json!({ "timeout": 2 })).await;
        assert!(!result.success, "IPv6 link-local literal must not succeed");
        assert_eq!(result.detail, Value::Null);
        assert!(!result.error.unwrap_or_default().is_empty());
    }

    #[tokio::test]
    async fn test_check_rejects_ipv6_nat64_metadata_literal() {
        // 64:ff9b::a9fe:a9fe is the NAT64-wrapped 169.254.169.254 metadata IP.
        let result = check("http://[64:ff9b::a9fe:a9fe]/", &json!({ "timeout": 2 })).await;
        assert!(!result.success, "NAT64 metadata literal must not succeed");
        assert_eq!(result.detail, Value::Null);
        assert!(!result.error.unwrap_or_default().is_empty());
    }

    #[tokio::test]
    async fn test_check_blocks_this_network_literal() {
        // 0.0.0.0/8 "this network" literal is blocked (octets[0] == 0).
        let result = check("http://0.0.0.0/", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_blocks_broadcast_literal() {
        // 255.255.255.255 broadcast literal is blocked by the monitor guard.
        let result = check("http://255.255.255.255/", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_blocks_loopback_on_custom_port() {
        // Any port is allowed by `validate_monitor_url`, so the non-standard port
        // passes URL validation and the address guard still blocks loopback. This
        // exercises `port_or_known_default` returning the explicit (non-80) port.
        let result = check("http://127.0.0.1:8443/health", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        let err = result.error.unwrap_or_default();
        assert!(
            err.contains("SSRF guard"),
            "loopback on a custom port should still be blocked, got: {err}"
        );
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_post_config_parsed_before_ssrf_block() {
        // A full POST config (method/body/headers/expected_status/keyword/timeout)
        // is parsed inside `check`, but the SSRF guard short-circuits on a blocked
        // loopback target before any request is built or sent. Confirms config
        // parsing with non-default values does not panic and the guard wins.
        let config = json!({
            "method": "post",
            "body": "{\"ping\":true}",
            "headers": { "Content-Type": "application/json" },
            "expected_status": [201, 204],
            "keyword": "ok",
            "keyword_exists": true,
            "timeout": 3
        });
        let result = check("http://127.0.0.1/api", &config).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
        // Validation-path failures carry a Null detail, not a status payload.
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_valid_headers_then_ssrf_block() {
        // Valid custom headers build successfully (the Ok arm of `build_headers`
        // inside `check`), then the loopback target is blocked by the SSRF guard.
        let config = json!({
            "headers": { "X-Custom": "value", "Accept": "text/plain" }
        });
        let result = check("http://127.0.0.1/", &config).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_https_scheme_loopback_blocked() {
        // An https loopback literal also passes scheme/credential validation but
        // is blocked at the address guard (port defaults to 443 here).
        let result = check("https://127.0.0.1/secure", &json!({ "timeout": 2 })).await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("SSRF guard"));
        assert_eq!(result.detail, Value::Null);
    }

    #[tokio::test]
    async fn test_check_empty_scheme_rejected() {
        // An empty/missing scheme fails `Url::parse` and is rejected during
        // validation before any network access.
        let result = check("://no-scheme.example", &json!({})).await;
        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.detail, Value::Null);
    }

    // ── keyword matching: case sensitivity and substring boundaries ───────────

    #[test]
    fn test_keyword_match_is_case_sensitive() {
        // `String::contains` is case-sensitive: a case mismatch is not a match.
        let body = "Status: OPERATIONAL";
        assert!(!body.contains("operational"));
        assert!(body.contains("OPERATIONAL"));
    }

    #[test]
    fn test_keyword_match_substring_within_word() {
        // Keyword matching is plain substring (no word-boundary requirement).
        let body = "maintenancewindow active";
        let keyword = "maintenance";
        let keyword_found = Some(body.contains(keyword));
        assert_eq!(keyword_found, Some(true));
    }

    #[test]
    fn test_keyword_match_empty_keyword_always_found() {
        // An empty keyword is a substring of every string, so it always matches.
        let body = "";
        let keyword = "";
        assert!(body.contains(keyword));
    }

    #[test]
    fn test_keyword_classification_absent_expected_absent_ok() {
        // keyword_exists=false and the keyword is genuinely absent => ok. This
        // covers the (false found == false expected) success case omitted above.
        let body = "all good";
        let keyword = Some("ERROR");
        let keyword_exists = false;
        let keyword_found = keyword.map(|kw| body.contains(kw));
        let keyword_ok = match (keyword, keyword_found) {
            (Some(_), Some(found)) => found == keyword_exists,
            (None, _) => true,
            _ => true,
        };
        assert_eq!(keyword_found, Some(false));
        assert!(keyword_ok, "absent keyword expected absent must pass");
    }

    // ── success composition: status + keyword combined ────────────────────────

    #[test]
    fn test_success_requires_both_status_and_keyword_ok() {
        // `success = status_ok && keyword_ok`: both true => success; either false
        // => failure. Mirrors the final composition in `check`.
        for (status_ok, keyword_ok, expected) in [
            (true, true, true),
            (true, false, false),
            (false, true, false),
            (false, false, false),
        ] {
            assert_eq!(status_ok && keyword_ok, expected);
        }
    }

    #[test]
    fn test_error_message_combines_status_and_keyword_failures() {
        // When both status and keyword fail, the reasons are joined with "; ".
        let status_code: u16 = 503;
        let expected_status: Vec<u16> = vec![200];
        let keyword = Some("healthy");
        let keyword_exists = true;
        let status_ok = expected_status.contains(&status_code);
        let keyword_ok = false;

        let mut reasons = Vec::new();
        if !status_ok {
            reasons.push(format!(
                "Status code {status_code} not in expected {expected_status:?}"
            ));
        }
        if !keyword_ok {
            if keyword_exists {
                reasons.push(format!(
                    "Keyword '{}' not found in response",
                    keyword.unwrap_or("")
                ));
            } else {
                reasons.push(format!(
                    "Keyword '{}' found in response but should be absent",
                    keyword.unwrap_or("")
                ));
            }
        }
        let msg = reasons.join("; ");
        assert!(msg.contains("Status code 503"));
        assert!(msg.contains("'healthy' not found"));
        assert!(msg.contains("; "), "two reasons must be joined with a separator");
    }

    #[test]
    fn test_status_classification_multiple_expected_codes() {
        // A status present in a multi-entry expected list classifies as ok.
        let expected_status: Vec<u16> = vec![200, 204, 301];
        assert!(expected_status.contains(&301));
        assert!(!expected_status.contains(&500));
    }

    #[test]
    fn test_timeout_default_and_custom_parsing() {
        // `timeout` defaults to 10s when absent and uses the configured value
        // otherwise (the line-44 parsing branch in `check`).
        let default = json!({})
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);
        assert_eq!(default, 10);

        let custom = json!({ "timeout": 25 })
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);
        assert_eq!(custom, 25);
    }

    // ── expected_status parsing: boundary array shapes ────────────────────────

    #[test]
    fn test_expected_status_parsing_empty_array() {
        // An explicit empty `expected_status` array parses to an empty Vec (NOT
        // the default [200]), so `.contains()` later rejects every status code.
        let config = json!({ "expected_status": [] });
        let expected_status: Vec<u16> = config
            .get("expected_status")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_u64().map(|n| n as u16))
                    .collect()
            })
            .unwrap_or_else(|| vec![200]);
        assert!(expected_status.is_empty(), "empty array stays empty, not defaulted");
        assert!(!expected_status.contains(&200), "no status is acceptable with an empty list");
    }

    #[test]
    fn test_expected_status_parsing_skips_negative_values() {
        // Negative JSON numbers fail `as_u64()` and are filtered out, leaving
        // only the valid unsigned codes.
        let config = json!({ "expected_status": [-1, 200, -404, 302] });
        let expected_status: Vec<u16> = config
            .get("expected_status")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_u64().map(|n| n as u16))
                    .collect()
            })
            .unwrap_or_else(|| vec![200]);
        assert_eq!(expected_status, vec![200, 302]);
    }

    #[test]
    fn test_expected_status_parsing_truncates_out_of_range() {
        // A value above u16::MAX passes `as_u64()` but `as u16` truncates it
        // (70000 & 0xFFFF == 4464); this pins the lossy-cast contract.
        let config = json!({ "expected_status": [70000] });
        let expected_status: Vec<u16> = config
            .get("expected_status")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_u64().map(|n| n as u16))
                    .collect()
            })
            .unwrap_or_else(|| vec![200]);
        // 70000 wraps to 70000 - 65536 = 4464 under the production `as u16` cast.
        assert_eq!(expected_status, vec![4464]);
    }

    #[test]
    fn test_expected_status_non_array_falls_back_to_default() {
        // `expected_status` present but not an array: `.as_array()` is None so
        // the whole `.map` is skipped and the [200] default applies.
        let config = json!({ "expected_status": 200 });
        let expected_status: Vec<u16> = config
            .get("expected_status")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_u64().map(|n| n as u16))
                    .collect()
            })
            .unwrap_or_else(|| vec![200]);
        assert_eq!(expected_status, vec![200]);
    }

    // ── build_headers: additional value-type branches ─────────────────────────

    #[test]
    fn test_build_headers_null_value_rejected() {
        // A JSON null header value is not a string, so `.as_str()` is None and
        // the "must be a string" error path fires.
        let config = json!({ "headers": { "X-Header": null } });
        let result = build_headers(&config);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("must be a string"),
            "a null header value should report a string-type error"
        );
    }

    #[test]
    fn test_build_headers_empty_string_value_ok() {
        // An empty-string header value is valid (HeaderValue accepts it).
        let config = json!({ "headers": { "X-Empty": "" } });
        let headers = build_headers(&config).unwrap();
        assert_eq!(headers.get("x-empty").unwrap(), "");
    }

    #[test]
    fn test_build_headers_first_invalid_entry_short_circuits() {
        // The loop returns on the first bad entry; with one valid and one invalid
        // header the overall result is the error (insertion order is unspecified,
        // so we only assert that it errors).
        let config = json!({
            "headers": {
                "X-Good": "ok",
                "X-Bad": 7
            }
        });
        let result = build_headers(&config);
        assert!(result.is_err(), "any invalid header entry makes the whole build fail");
    }

    // ── method / body config parsing branches ─────────────────────────────────

    #[test]
    fn test_body_str_present_and_absent() {
        // `body` is read as an optional &str: present => Some, absent => None.
        let with_body = json!({ "body": "payload" });
        assert_eq!(with_body.get("body").and_then(|v| v.as_str()), Some("payload"));

        let without_body = json!({ "method": "POST" });
        assert_eq!(without_body.get("body").and_then(|v| v.as_str()), None);
    }

    #[test]
    fn test_method_unknown_value_uppercased() {
        // An arbitrary method string is uppercased verbatim; the `other =>`
        // rejection arm in `check` would only be reached after the SSRF guard, so
        // here we just pin the normalization step that feeds it.
        let cfg = json!({ "method": "patch" });
        let method = cfg
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        assert_eq!(method, "PATCH");
    }

    #[test]
    fn test_method_non_string_falls_back_to_get() {
        // A non-string `method` value fails `.as_str()` and defaults to GET.
        let cfg = json!({ "method": 123 });
        let method = cfg
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        assert_eq!(method, "GET");
    }

    // ── keyword classification: fall-through arm and keyword=None error label ──

    #[test]
    fn test_keyword_classification_fallthrough_arm_is_ok() {
        // The `_ => true` arm covers the logically-impossible (Some keyword, None
        // found) shape; it must classify as ok so an inconsistent state never
        // fails the check spuriously.
        let keyword: Option<&str> = Some("x");
        let keyword_found: Option<bool> = None;
        let keyword_exists = true;
        let keyword_ok = match (keyword, keyword_found) {
            (Some(_), Some(found)) => found == keyword_exists,
            (None, _) => true,
            _ => true,
        };
        assert!(keyword_ok, "the catch-all arm defaults to ok");
    }

    #[test]
    fn test_error_message_keyword_none_uses_empty_label() {
        // When keyword is None but the keyword branch is somehow hit, the message
        // falls back to an empty-quoted label via `keyword.unwrap_or("")`.
        let keyword: Option<&str> = None;
        let keyword_exists = true;
        let msg = if keyword_exists {
            format!("Keyword '{}' not found in response", keyword.unwrap_or(""))
        } else {
            format!(
                "Keyword '{}' found in response but should be absent",
                keyword.unwrap_or("")
            )
        };
        assert_eq!(msg, "Keyword '' not found in response");
    }

    // ── success path: no error message when both checks pass ──────────────────

    #[test]
    fn test_no_error_when_success() {
        // On success the `error` field is None (the else branch of the assembly).
        let status_ok = true;
        let keyword_ok = true;
        let success = status_ok && keyword_ok;
        let error: Option<String> = if success {
            None
        } else {
            Some("unused".to_string())
        };
        assert!(error.is_none(), "a passing check carries no error message");
    }

    // ── port defaulting reasoning for the SSRF pin ────────────────────────────

    #[test]
    fn test_port_or_known_default_for_http_and_https() {
        // `check` derives the resolve port from `url.port_or_known_default()`:
        // http => 80, https => 443, explicit port => that port.
        let http = url::Url::parse("http://example.com/").unwrap();
        assert_eq!(http.port_or_known_default(), Some(80));

        let https = url::Url::parse("https://example.com/").unwrap();
        assert_eq!(https.port_or_known_default(), Some(443));

        let explicit = url::Url::parse("http://example.com:8443/").unwrap();
        assert_eq!(explicit.port_or_known_default(), Some(8443));
    }
}
