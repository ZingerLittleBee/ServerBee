#![allow(dead_code)]

pub mod cli;
pub mod store;

#[allow(unused_imports)]
pub use store::{CapabilityGrantStore, GrantRecord};

/// Parse a human duration (`90s`, `30m`, `2h`, `1d`) into seconds. Must be a
/// positive integer followed by a single unit char. Footgun-guard only.
pub fn parse_duration_secs(input: &str) -> anyhow::Result<i64> {
    let trimmed = input.trim();
    if trimmed.len() < 2 {
        anyhow::bail!("invalid duration '{input}': expected <number><s|m|h|d>, e.g. 30m");
    }
    let (num, unit) = trimmed.split_at(trimmed.len() - 1);
    let value: i64 = num
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid duration '{input}': '{num}' is not an integer"))?;
    if value <= 0 {
        anyhow::bail!("invalid duration '{input}': must be positive");
    }
    let secs = match unit {
        "s" => Some(value),
        "m" => value.checked_mul(60),
        "h" => value.checked_mul(3600),
        "d" => value.checked_mul(86_400),
        other => anyhow::bail!("invalid duration unit '{other}': expected s, m, h, or d"),
    }
    .ok_or_else(|| anyhow::anyhow!("duration '{input}' overflows"))?;
    Ok(secs)
}

#[cfg(test)]
mod duration_tests {
    use super::parse_duration_secs;

    #[test]
    fn parses_each_unit() {
        assert_eq!(parse_duration_secs("90s").unwrap(), 90);
        assert_eq!(parse_duration_secs("30m").unwrap(), 1800);
        assert_eq!(parse_duration_secs("2h").unwrap(), 7200);
        assert_eq!(parse_duration_secs("1d").unwrap(), 86_400);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse_duration_secs("").is_err());
        assert!(parse_duration_secs("h").is_err());
        assert!(parse_duration_secs("0m").is_err());
        assert!(parse_duration_secs("-5m").is_err());
        assert!(parse_duration_secs("10y").is_err());
        assert!(parse_duration_secs("abcm").is_err());
        // Overflow path: a valid i64 that overflows the `checked_mul` by 60 ("m").
        assert!(parse_duration_secs("9999999999999999999s").is_err());
        assert!(parse_duration_secs("999999999999999999m").is_err());
    }
}
