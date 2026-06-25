use std::time::Instant;

use chrono::{NaiveDate, NaiveDateTime, Utc};
use reqwest::Url;
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
const UNSUPPORTED_WHOIS_TLDS: &[&str] = &["app", "dev", "page"];

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

    let lookup_target = match normalize_lookup_target(target) {
        Ok(target) => target,
        Err(error) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            return CheckResult {
                success: false,
                latency: Some(latency),
                detail: Value::Null,
                error: Some(error),
            };
        }
    };

    if let Some(error) = unsupported_tld_error(&lookup_target) {
        let latency = start.elapsed().as_secs_f64() * 1000.0;
        return CheckResult {
            success: false,
            latency: Some(latency),
            detail: Value::Null,
            error: Some(error),
        };
    }

    // Try whois-rust crate first
    let whois_text = match query_whois(&lookup_target).await {
        Ok(text) => text,
        Err(crate_err) => {
            // Fall back to system `whois` command
            match query_whois_system(&lookup_target).await {
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

fn normalize_lookup_target(target: &str) -> Result<String, String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("WHOIS target is empty.".to_string());
    }

    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };

    let url = Url::parse(&candidate)
        .map_err(|_| "WHOIS target must be a domain, URL, or host:port.".to_string())?;

    let host = url
        .host_str()
        .ok_or_else(|| "WHOIS target must include a host name.".to_string())?;
    let normalized = host.trim().trim_end_matches('.').to_ascii_lowercase();

    if normalized.is_empty() {
        return Err("WHOIS target must include a host name.".to_string());
    }

    Ok(normalized)
}

fn unsupported_tld_error(target: &str) -> Option<String> {
    let tld = target.rsplit('.').next()?;

    if UNSUPPORTED_WHOIS_TLDS.contains(&tld) {
        return Some(format!(
            ".{tld} domains do not expose a standard WHOIS service in this monitor. Use an SSL monitor for {target} instead."
        ));
    }

    None
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
    use chrono::Timelike;

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

    #[test]
    fn test_normalize_lookup_target_from_url() {
        let result = normalize_lookup_target("https://demo.example.com/path").unwrap();
        assert_eq!(result, "demo.example.com");
    }

    #[test]
    fn test_normalize_lookup_target_from_host_port() {
        let result = normalize_lookup_target("demo.example.com:8443").unwrap();
        assert_eq!(result, "demo.example.com");
    }

    #[tokio::test]
    async fn test_check_returns_clear_error_for_unsupported_google_registry_tld() {
        let result = check("https://demo.serverbee.app/path", &json!({})).await;

        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
        assert_eq!(
            result.error,
            Some(
                ".app domains do not expose a standard WHOIS service in this monitor. Use an SSL monitor for demo.serverbee.app instead.".to_string()
            )
        );
    }

    // ---- normalize_lookup_target branch coverage ----

    #[test]
    fn test_normalize_lookup_target_empty() {
        // Empty / whitespace-only input hits the early empty-trim error branch.
        let err = normalize_lookup_target("   ").unwrap_err();
        assert_eq!(err, "WHOIS target is empty.");
    }

    #[test]
    fn test_normalize_lookup_target_bare_domain() {
        // Bare domain gets an https:// scheme prepended before URL parsing.
        let result = normalize_lookup_target("EXAMPLE.COM").unwrap();
        assert_eq!(result, "example.com");
    }

    #[test]
    fn test_normalize_lookup_target_trailing_dot() {
        // Trailing root-label dot is stripped and host is lowercased.
        let result = normalize_lookup_target("Example.COM.").unwrap();
        assert_eq!(result, "example.com");
    }

    #[test]
    fn test_normalize_lookup_target_with_scheme_preserved() {
        // Input that already contains "://" is used verbatim as the parse candidate.
        let result = normalize_lookup_target("http://demo.example.com").unwrap();
        assert_eq!(result, "demo.example.com");
    }

    #[test]
    fn test_normalize_lookup_target_invalid_url() {
        // A string the URL parser rejects yields the domain/URL/host:port error.
        let err = normalize_lookup_target("http://").unwrap_err();
        assert_eq!(
            err,
            "WHOIS target must be a domain, URL, or host:port."
        );
    }

    #[test]
    fn test_normalize_lookup_target_no_host() {
        // A non-special scheme with an empty authority parses successfully but
        // exposes an empty host, hitting the missing-host branch. The input
        // already contains "://" so it is used verbatim (no https:// prefix).
        let err = normalize_lookup_target("foo://").unwrap_err();
        assert_eq!(err, "WHOIS target must include a host name.");
    }

    // ---- unsupported_tld_error branch coverage ----

    #[test]
    fn test_unsupported_tld_error_supported() {
        // A standard TLD must not be flagged as unsupported.
        assert!(unsupported_tld_error("example.com").is_none());
    }

    #[test]
    fn test_unsupported_tld_error_unsupported_dev() {
        let err = unsupported_tld_error("demo.example.dev").unwrap();
        assert!(err.contains(".dev domains"));
        assert!(err.contains("demo.example.dev"));
    }

    #[test]
    fn test_unsupported_tld_error_unsupported_page() {
        let err = unsupported_tld_error("foo.page").unwrap();
        assert!(err.contains(".page domains"));
    }

    // ---- parse_expiry_date branch coverage ----

    #[test]
    fn test_parse_expiry_date_case_insensitive() {
        // Lowercased field label still matches via the case-insensitive fallback.
        let text = "expiration date: 2030-01-02\n";
        let result = parse_expiry_date(text);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().date(),
            NaiveDate::from_ymd_opt(2030, 1, 2).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_registrar_expiration() {
        let text = "Registrar Registration Expiration Date: 2028-03-04T00:00:00Z\n";
        let result = parse_expiry_date(text);
        assert_eq!(
            result.unwrap().date(),
            NaiveDate::from_ymd_opt(2028, 3, 4).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_indented_line() {
        // Leading whitespace before the field label is trimmed before matching.
        let text = "    Expiry Date: 2027-07-07\n";
        let result = parse_expiry_date(text);
        assert_eq!(
            result.unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 7, 7).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_pattern_present_but_unparsable_date() {
        // Field label matches but the value is not a date; loop continues and
        // ultimately returns None.
        let text = "Expiry Date: never\n";
        let result = parse_expiry_date(text);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_expiry_date_picks_first_valid() {
        // Multiple expiry lines: the first parseable one wins.
        let text = "Expiry Date: 2024-01-01\nRegistry Expiry Date: 2025-01-01\n";
        let result = parse_expiry_date(text);
        assert_eq!(
            result.unwrap().date(),
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
        );
    }

    // ---- parse_date_string branch coverage ----

    #[test]
    fn test_parse_date_string_datetime_with_fraction() {
        let dt = parse_date_string("2025-08-15T04:00:00.123Z").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_date_string_datetime_no_tz() {
        let dt = parse_date_string("2025-08-15T04:00:00").unwrap();
        assert_eq!(dt.time().hour(), 4);
    }

    #[test]
    fn test_parse_date_string_space_separated_datetime() {
        let dt = parse_date_string("2025-08-15 04:30:00").unwrap();
        assert_eq!(dt.time().minute(), 30);
    }

    #[test]
    fn test_parse_date_string_slash_datetime() {
        let dt = parse_date_string("2025/08/15 12:00:00").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_date_string_dmy_month_name_datetime() {
        let dt = parse_date_string("15-Aug-2025 04:00:00").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_date_string_dotted_datetime() {
        let dt = parse_date_string("15.08.2025 04:00:00").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_date_string_strips_utc_suffix() {
        // " UTC" suffix is trimmed so the date-only format applies.
        let dt = parse_date_string("2025-08-15 UTC").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_date_string_strips_gmt_suffix() {
        let dt = parse_date_string("2025-08-15 GMT").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_date_string_dotted_ymd_date() {
        let dt = parse_date_string("2025.08.15").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn test_parse_date_string_empty() {
        assert!(parse_date_string("").is_none());
    }

    // ---- parse_registrar branch coverage ----

    #[test]
    fn test_parse_registrar_sponsoring() {
        let text = "Sponsoring Registrar: Example Registrar Inc.\n";
        assert_eq!(
            parse_registrar(text),
            Some("Example Registrar Inc.".to_string())
        );
    }

    #[test]
    fn test_parse_registrar_lowercase_pattern() {
        let text = "registrar: lower-case-registrar\n";
        assert_eq!(
            parse_registrar(text),
            Some("lower-case-registrar".to_string())
        );
    }

    #[test]
    fn test_parse_registrar_skips_empty_value() {
        // An empty value after the label must be skipped; the next line wins.
        let text = "Registrar:\nSponsoring Registrar: Real Registrar\n";
        assert_eq!(
            parse_registrar(text),
            Some("Real Registrar".to_string())
        );
    }

    // ---- truncate_whois boundary coverage ----

    #[test]
    fn test_truncate_whois_exact_length() {
        // Length exactly equal to max_len returns the text unchanged.
        let text = "abcd";
        assert_eq!(truncate_whois(text, 4), "abcd");
    }

    // ---- check() error-path coverage (no network reached) ----

    #[tokio::test]
    async fn test_check_empty_target_returns_error() {
        // Empty target fails at normalization before any network call.
        let result = check("   ", &json!({})).await;
        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
        assert_eq!(result.error, Some("WHOIS target is empty.".to_string()));
        assert!(result.latency.is_some());
    }

    #[tokio::test]
    async fn test_check_unsupported_tld_respects_normalization() {
        // .dev TLD is rejected after normalization, before any network call.
        let result = check("foo.bar.dev", &json!({})).await;
        assert!(!result.success);
        assert_eq!(result.detail, Value::Null);
        let err = result.error.unwrap();
        assert!(err.contains(".dev domains"));
        assert!(err.contains("foo.bar.dev"));
    }

    // ---- additional EXPIRY_PATTERNS coverage (each label variant) ----

    #[test]
    fn test_parse_expiry_date_registry_expiry_date() {
        // "Registry Expiry Date:" pattern with an ISO timestamp.
        let text = "Registry Expiry Date: 2026-09-09T04:00:00Z\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2026, 9, 9).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_plain_expiry_date_pattern() {
        // "Expiry Date:" (distinct from the lowercase "Expiry date:" entry).
        let text = "Expiry Date: 2026-05-06\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2026, 5, 6).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_lowercase_expiry_date_label() {
        // "Expiry date:" entry (different casing than "Expiry Date:").
        let text = "Expiry date: 2026-04-03\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2026, 4, 3).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_expire_label() {
        // "expire:" pattern used by some ccTLD registries.
        let text = "expire: 2027-01-15\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 1, 15).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_expires_label() {
        // "expires:" pattern.
        let text = "expires: 2027-02-16\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 2, 16).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_expires_on_label() {
        // "Expires On:" pattern.
        let text = "Expires On: 2027-03-17\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 3, 17).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_lowercase_expiration_date_label() {
        // "Expiration date:" entry (distinct casing from "Expiration Date:").
        let text = "Expiration date: 2027-04-18\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 4, 18).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_renewal_date_label() {
        // "renewal date:" pattern.
        let text = "renewal date: 2027-05-19\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 5, 19).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_free_date_label() {
        // "free-date:" pattern.
        let text = "free-date: 2027-06-20\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 6, 20).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_domain_expiration_date_label() {
        // "Domain Expiration Date:" pattern.
        let text = "Domain Expiration Date: 2027-07-21\n";
        assert_eq!(
            parse_expiry_date(text).unwrap().date(),
            NaiveDate::from_ymd_opt(2027, 7, 21).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_case_insensitive_no_match_branch() {
        // Line is not blank but never starts with any pattern, so the
        // case-insensitive fallback returns None for every pattern and the
        // overall result is None. Exercises the `else None` arm.
        let text = "Some Other Field: 2030-01-02\nDomain Status: ok\n";
        assert!(parse_expiry_date(text).is_none());
    }

    #[test]
    fn test_parse_expiry_date_empty_text() {
        // Empty input produces no lines and yields None.
        assert!(parse_expiry_date("").is_none());
    }

    // ---- parse_date_string: timezone-abbreviation datetime format ----

    #[test]
    fn test_parse_date_string_datetime_with_tz_abbrev() {
        // "%Y-%m-%d %H:%M:%S %Z" format with a textual timezone token that is
        // NOT one of the stripped " UTC"/" GMT" suffixes, so the %Z format is
        // actually exercised rather than the bare datetime format.
        let dt = parse_date_string("2025-08-15 04:00:00 CET").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
        assert_eq!(dt.time().hour(), 4);
    }

    #[test]
    fn test_parse_date_string_whitespace_only_is_none() {
        // Whitespace-only input trims to empty and matches no format.
        assert!(parse_date_string("   ").is_none());
    }

    // ---- normalize_lookup_target: host present but normalizes to empty ----

    #[test]
    fn test_normalize_lookup_target_host_only_dot() {
        // A bare "." host (root label) parses but trims to an empty normalized
        // host, hitting the second missing-host check after trimming.
        let err = normalize_lookup_target(".").err().expect("expected error");
        assert_eq!(err, "WHOIS target must include a host name.");
    }

    #[test]
    fn test_normalize_lookup_target_uppercase_with_scheme() {
        // Scheme-prefixed input is lowercased and the trailing dot stripped.
        let result = normalize_lookup_target("HTTPS://Demo.Example.COM.").unwrap();
        assert_eq!(result, "demo.example.com");
    }

    // ---- unsupported_tld_error: domain with no dot / case sensitivity ----

    #[test]
    fn test_unsupported_tld_error_no_dot_uses_whole_string() {
        // With no dot, rsplit yields the whole token as the "TLD"; "app" with
        // no leading label is still flagged as unsupported.
        let err = unsupported_tld_error("app").unwrap();
        assert!(err.contains(".app domains"));
    }

    #[test]
    fn test_unsupported_tld_error_no_dot_supported_token() {
        // A bare label that is not in the unsupported set returns None.
        assert!(unsupported_tld_error("localhost").is_none());
    }

    #[test]
    fn test_unsupported_tld_error_empty_string() {
        // Empty input: rsplit('.').next() is Some(""), which is not in the
        // unsupported set, so the function returns None without panicking.
        assert!(unsupported_tld_error("").is_none());
    }

    // ---- truncate_whois: one-over and zero-length boundaries ----

    #[test]
    fn test_truncate_whois_one_over_max() {
        // Length exactly one over max_len triggers truncation to max_len + "...".
        let text = "a".repeat(11);
        let truncated = truncate_whois(&text, 10);
        assert_eq!(truncated, format!("{}...", "a".repeat(10)));
    }

    #[test]
    fn test_truncate_whois_zero_max_with_text() {
        // max_len of 0 with non-empty text truncates to just the ellipsis.
        assert_eq!(truncate_whois("abc", 0), "...");
    }

    #[test]
    fn test_truncate_whois_empty_text() {
        // Empty text is always <= max_len and is returned unchanged.
        assert_eq!(truncate_whois("", 10), "");
    }

    // ---- parse_registrar: first-match ordering and CRLF handling ----

    #[test]
    fn test_parse_registrar_first_pattern_wins() {
        // The first matching line in document order is returned even when a
        // later "Sponsoring Registrar:" line is also present.
        let text = "Registrar: First Registrar\nSponsoring Registrar: Second\n";
        assert_eq!(
            parse_registrar(text),
            Some("First Registrar".to_string())
        );
    }

    #[test]
    fn test_parse_registrar_crlf_value_trimmed() {
        // A CRLF line ending leaves a trailing "\r" that the inner trim() must
        // strip from the captured registrar value.
        let text = "Registrar: Trailing CR Registrar\r\n";
        assert_eq!(
            parse_registrar(text),
            Some("Trailing CR Registrar".to_string())
        );
    }

    #[test]
    fn test_parse_registrar_whitespace_only_value_skipped() {
        // A label followed only by spaces trims to empty and is skipped, so the
        // later valid line is returned instead.
        let text = "Registrar:    \nSponsoring Registrar: Backup Registrar\n";
        assert_eq!(
            parse_registrar(text),
            Some("Backup Registrar".to_string())
        );
    }

    // ---- parse_date_string: month-name and dotted date-only formats ----

    #[test]
    fn test_parse_date_string_dmy_month_name_date_only() {
        // "%d-%b-%Y" date-only format (no time) maps to midnight.
        let dt = parse_date_string("09-Sep-2026").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2026, 9, 9).unwrap());
        assert_eq!(dt.time().hour(), 0);
        assert_eq!(dt.time().minute(), 0);
        assert_eq!(dt.time().second(), 0);
    }

    #[test]
    fn test_parse_date_string_dotted_dmy_date_only() {
        // "%d.%m.%Y" date-only format resolves day/month/year correctly.
        let dt = parse_date_string("01.02.2026").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
        assert_eq!(dt.time().hour(), 0);
    }

    #[test]
    fn test_parse_date_string_slash_date_only_midnight() {
        // "%Y/%m/%d" date-only format yields a midnight datetime.
        let dt = parse_date_string("2026/03/04").unwrap();
        assert_eq!(dt.date(), NaiveDate::from_ymd_opt(2026, 3, 4).unwrap());
        assert_eq!(dt.time(), chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    }

    #[test]
    fn test_parse_date_string_partial_date_not_accepted() {
        // A year-month-only string matches none of the full date/datetime
        // formats and returns None.
        assert!(parse_date_string("2026-08").is_none());
    }

    #[test]
    fn test_parse_date_string_strips_only_trailing_tz_suffix() {
        // " UTC"/" GMT" trimming is suffix-only; a leading "UTC" token leaves an
        // unparsable string and yields None.
        assert!(parse_date_string("UTC 2026-08-15").is_none());
    }

    // ---- unsupported_tld_error: case sensitivity of the TLD comparison ----

    #[test]
    fn test_unsupported_tld_error_uppercase_tld_not_matched() {
        // The comparison is case-sensitive against the lowercase unsupported
        // set; an uppercase ".APP" token is not flagged (normalization would
        // have lowercased it before this is called in check()).
        assert!(unsupported_tld_error("demo.example.APP").is_none());
    }

    #[test]
    fn test_unsupported_tld_error_substring_tld_not_matched() {
        // A TLD that merely contains an unsupported token (e.g. "apple") must
        // not match the exact-equality membership check.
        assert!(unsupported_tld_error("foo.apple").is_none());
    }

    // ---- parse_expiry_date: case-insensitive path with valid date value ----

    #[test]
    fn test_parse_expiry_date_uppercase_label_case_insensitive() {
        // An all-uppercase label only matches via the case-insensitive fallback
        // (exact strip_prefix fails) and still extracts a valid date.
        let text = "EXPIRES: 2029-10-11\n";
        let result = parse_expiry_date(text);
        assert_eq!(
            result.unwrap().date(),
            NaiveDate::from_ymd_opt(2029, 10, 11).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_case_insensitive_match_unparsable_continues() {
        // A case-insensitive label match whose value is not a date must not
        // short-circuit; a later valid expiry line is still found.
        let text = "EXPIRES: forever\nRegistry Expiry Date: 2031-12-25\n";
        let result = parse_expiry_date(text);
        assert_eq!(
            result.unwrap().date(),
            NaiveDate::from_ymd_opt(2031, 12, 25).unwrap()
        );
    }

    #[test]
    fn test_parse_expiry_date_crlf_lines() {
        // CRLF-terminated WHOIS text is split per-line and trimmed before the
        // pattern match, so the date is still extracted.
        let text = "Domain: example.com\r\nRegistry Expiry Date: 2030-06-30T00:00:00Z\r\n";
        let result = parse_expiry_date(text);
        assert_eq!(
            result.unwrap().date(),
            NaiveDate::from_ymd_opt(2030, 6, 30).unwrap()
        );
    }

    // ---- normalize_lookup_target: host:port with explicit scheme ----

    #[test]
    fn test_normalize_lookup_target_scheme_with_port_drops_port() {
        // A full URL with an explicit port returns only the lowercased host.
        let result = normalize_lookup_target("https://Demo.Example.com:8443/x").unwrap();
        assert_eq!(result, "demo.example.com");
    }

    #[test]
    fn test_normalize_lookup_target_ipv4_host() {
        // A bare IPv4 literal is a valid host and is returned verbatim.
        let result = normalize_lookup_target("192.0.2.1").unwrap();
        assert_eq!(result, "192.0.2.1");
    }

    // ---- check(): None-expiry detail shape is unreachable offline; verified
    // via parse helpers above. Documented in summary as network-only. ----
}
