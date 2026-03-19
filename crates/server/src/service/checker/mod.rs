pub mod dns;
pub mod http_keyword;
pub mod ssl;
pub mod tcp;
pub mod whois;

use serde_json::Value;

/// The result of a service monitor check.
pub struct CheckResult {
    /// Whether the check passed.
    pub success: bool,
    /// Round-trip latency in milliseconds, if measurable.
    pub latency: Option<f64>,
    /// Checker-specific detail payload.
    pub detail: Value,
    /// Human-readable error message on failure.
    pub error: Option<String>,
}

/// Dispatch a check to the appropriate checker implementation.
pub async fn run_check(monitor_type: &str, target: &str, config: &Value) -> CheckResult {
    match monitor_type {
        "ssl" => ssl::check(target, config).await,
        "dns" => dns::check(target, config).await,
        "http_keyword" => http_keyword::check(target, config).await,
        "tcp" => tcp::check(target, config).await,
        "whois" => whois::check(target, config).await,
        _ => CheckResult {
            success: false,
            latency: None,
            detail: Value::Null,
            error: Some(format!("Unknown monitor type: {monitor_type}")),
        },
    }
}
