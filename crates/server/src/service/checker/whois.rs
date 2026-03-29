use std::time::Instant;

use chrono::{NaiveDate, NaiveDateTime, Utc};
use serde_json::{Value, json};

use super::CheckResult;

/// Known WHOIS expiry date field patterns.
const EXPIRY_PATTERNS: &[&str] = &[
    "Registry Expiry Date:",
    "Registrar Registration Expiration Date:",
    "Expiration Date:",
    "Expiry Date:",
    "paid-till:",
    "Expiry date:",
    "expire:",
    "expires:",
    "Expires On:",
    "Expiration date:",
    "renewal date:",
    "free-date:",
    "Domain Expiration Date:",
];

/// Known WHOIS registrar field patterns.
const REGISTRAR_PATTERNS: &[&str] = &["Registrar:", "Sponsoring Registrar:", "registrar:"];

/// Check WHOIS information for a domain.
///
/// Config options:
/// - `warning_days`: days before expiry to warn (default 30)
/// - `critical_days`: days before expiry to consider failure (default 7)
pub async fn check(target: &str, config: &Value) -> CheckResult {
    let start = Instant::now();

    let warning_days = config
        .get("warning_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(30);
    let critical_days = config
        .get("critical_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(7);

    // Try whois-rust crate first
    let whois_text = match query_whois(target).await {
        Ok(text) => text,
        Err(crate_err) => {
            // Fall back to system `whois` command
            match query_whois_system(target).await {
                Ok(text) => text,
                Err(sys_err) => {
                    let latency = start.elapsed().as_secs_f64() * 1000.0;
                    return CheckResult {
                        success: false,
                        latency: Some(latency),
                        detail: Value::Null,
                        error: Some(format!(
                            "WHOIS lookup failed. Crate: {crate_err}; System: {sys_err}"
                        )),
                    };
                }
            }
        }
    };

    let latency = start.elapsed().as_secs_f64() * 1000.0;

    // Parse expiry date
    let expiry_date = parse_expiry_date(&whois_text);
    let registrar = parse_registrar(&whois_text);

    match expiry_date {
        Some(expiry) => {
            let now = Utc::now().naive_utc();
            let days_remaining = (expiry - now).num_days();
            let success = days_remaining > critical_days;

            let mut detail = json!({
                "registrar": registrar.unwrap_or_default(),
                "expiry_date": expiry.format("%Y-%m-%d").to_string(),
                "days_remaining": days_remaining,
            });

            if days_remaining <= warning_days {
                detail["warning"] = json!(format!(
                    "Domain expires in {days_remaining} days (warning threshold: {warning_days})"
                ));
            }

            let error = if !success {
                Some(format!(
                    "Domain expires in {days_remaining} days (critical threshold: {critical_days})"
                ))
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
        None => CheckResult {
            success: false,
            latency: Some(latency),
            detail: json!({
                "registrar": registrar.unwrap_or_default(),
                "expiry_date": null,
                "days_remaining": null,
                "raw_excerpt": truncate_whois(&whois_text, 500),
            }),
            error: Some("Could not parse expiry date from WHOIS response".to_string()),
        },
    }
}

/// Query WHOIS using the `whois-rust` crate (runs blocking I/O on a spawn_blocking thread).
async fn query_whois(target: &str) -> Result<String, String> {
    let target = target.to_string();
    tokio::task::spawn_blocking(move || {
        let whois = whois_rust::WhoIs::from_string(include_str!("whois_servers.json"))
            .map_err(|e| format!("Failed to initialize WHOIS client: {e}"))?;
        whois
            .lookup(
                whois_rust::WhoIsLookupOptions::from_string(&target).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("WHOIS lookup error: {e}"))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Fallback: query using the system `whois` command.
async fn query_whois_system(target: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("whois")
        .arg(target)
        .output()
        .await
        .map_err(|e| format!("Failed to execute whois command: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("whois command failed: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parse an expiry date from WHOIS response text.
fn parse_expiry_date(text: &str) -> Option<NaiveDateTime> {
    for line in text.lines() {
        let trimmed = line.trim();
        for pattern in EXPIRY_PATTERNS {
            if let Some(date_str) = trimmed.strip_prefix(pattern).or_else(|| {
                // Case-insensitive match
                let lower = trimmed.to_lowercase();
                let pat_lower = pattern.to_lowercase();
                if lower.starts_with(&pat_lower) {
                    Some(&trimmed[pattern.len()..])
                } else {
                    None
                }
            }) {
                let date_str = date_str.trim();
                if let Some(dt) = parse_date_string(date_str) {
                    return Some(dt);
                }
            }
        }
    }
    None
}

/// Try multiple date formats to parse a date string.
fn parse_date_string(s: &str) -> Option<NaiveDateTime> {
    // Common WHOIS date formats
    let datetime_formats = [
        "%Y-%m-%dT%H:%M:%S%.fZ",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S %Z",
        "%Y/%m/%d %H:%M:%S",
        "%d-%b-%Y %H:%M:%S",
        "%d.%m.%Y %H:%M:%S",
    ];

    let date_formats = ["%Y-%m-%d", "%Y/%m/%d", "%d-%b-%Y", "%d.%m.%Y", "%Y.%m.%d"];

    // Strip trailing timezone info like " UTC" for more flexible parsing
    let cleaned = s.trim_end_matches(" UTC").trim_end_matches(" GMT").trim();

    for fmt in &datetime_formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, fmt) {
            return Some(dt);
        }
    }

    for fmt in &date_formats {
        if let Ok(d) = NaiveDate::parse_from_str(cleaned, fmt) {
            return d.and_hms_opt(0, 0, 0);
        }
    }

    None
}

/// Parse the registrar from WHOIS response text.
fn parse_registrar(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        for pattern in REGISTRAR_PATTERNS {
            if let Some(value) = trimmed.strip_prefix(pattern) {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Truncate WHOIS response for inclusion in error details.
fn truncate_whois(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expiry_date_iso() {
        let text = "Registry Expiry Date: 2025-08-15T04:00:00Z\n";
        let result = parse_expiry_date(text);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_expiry_date_simple() {
        let text = "Expiration Date: 2025-12-31\n";
        let result = parse_expiry_date(text);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn test_parse_expiry_date_paid_till() {
        let text = "paid-till: 2025.06.15\n";
        let result = parse_expiry_date(text);
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_expiry_date_not_found() {
        let text = "Domain Name: example.com\nRegistrar: Test Registrar\n";
        let result = parse_expiry_date(text);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_registrar() {
        let text = "Registrar: GoDaddy.com, LLC\nExpiry Date: 2025-01-01\n";
        let result = parse_registrar(text);
        assert_eq!(result, Some("GoDaddy.com, LLC".to_string()));
    }

    #[test]
    fn test_parse_registrar_not_found() {
        let text = "Domain Name: example.com\n";
        let result = parse_registrar(text);
        assert!(result.is_none());
    }

    #[test]
    fn test_truncate_whois_short() {
        let text = "short text";
        assert_eq!(truncate_whois(text, 100), "short text");
    }

    #[test]
    fn test_truncate_whois_long() {
        let text = "a".repeat(600);
        let truncated = truncate_whois(&text, 500);
        assert_eq!(truncated.len(), 503); // 500 + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_parse_date_string_variants() {
        assert!(parse_date_string("2025-08-15T04:00:00Z").is_some());
        assert!(parse_date_string("2025-08-15").is_some());
        assert!(parse_date_string("2025/08/15").is_some());
        assert!(parse_date_string("15-Aug-2025").is_some());
        assert!(parse_date_string("15.08.2025").is_some());
        assert!(parse_date_string("not-a-date").is_none());
    }
}
