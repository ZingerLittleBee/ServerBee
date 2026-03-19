use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};

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

    let timeout_secs = config
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(10);

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

    // Build the HTTP client
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .danger_accept_invalid_certs(false)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("Failed to build HTTP client: {e}")),
            };
        }
    };

    // Build the request
    let mut request = match method.as_str() {
        "GET" => client.get(target),
        "POST" => {
            let mut req = client.post(target);
            if let Some(body) = body_str {
                req = req.body(body.to_string());
            }
            req
        }
        other => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(format!("Unsupported HTTP method: {other}")),
            };
        }
    };

    request = request.headers(custom_headers);

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

    let status_code = response.status().as_u16();

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
