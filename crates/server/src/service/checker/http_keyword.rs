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
    }
}
