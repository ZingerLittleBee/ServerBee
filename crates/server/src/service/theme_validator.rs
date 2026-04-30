use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::error::AppError;

pub const REQUIRED_VARS: &[&str] = &[
    "background",
    "foreground",
    "card",
    "card-foreground",
    "popover",
    "popover-foreground",
    "primary",
    "primary-foreground",
    "secondary",
    "secondary-foreground",
    "muted",
    "muted-foreground",
    "accent",
    "accent-foreground",
    "destructive",
    "border",
    "input",
    "ring",
    "chart-1",
    "chart-2",
    "chart-3",
    "chart-4",
    "chart-5",
    "sidebar",
    "sidebar-foreground",
    "sidebar-primary",
    "sidebar-primary-foreground",
    "sidebar-accent",
    "sidebar-accent-foreground",
    "sidebar-border",
    "sidebar-ring",
];

static OKLCH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*/\s*([\d.]+)(%)?)?\s*\)$")
        .expect("static regex")
});

pub type VarMap = HashMap<String, String>;

pub fn validate_var_map(map: &VarMap) -> Result<(), AppError> {
    let required: HashSet<&str> = REQUIRED_VARS.iter().copied().collect();
    let actual: HashSet<&str> = map.keys().map(|s| s.as_str()).collect();

    if let Some(missing) = required.difference(&actual).next() {
        return Err(AppError::Validation(format!("missing variable: {missing}")));
    }
    if let Some(extra) = actual.difference(&required).next() {
        return Err(AppError::Validation(format!("unknown variable: {extra}")));
    }

    for key in REQUIRED_VARS {
        let Some(value) = map.get(*key) else {
            return Err(AppError::Validation(format!("missing variable: {key}")));
        };
        validate_oklch_value(key, value)?;
    }

    Ok(())
}

fn validate_oklch_value(key: &str, value: &str) -> Result<(), AppError> {
    let Some(captures) = OKLCH_RE.captures(value) else {
        return Err(AppError::Validation(format!(
            "{key} must be an oklch(L C H) value"
        )));
    };

    let lightness = parse_capture(key, value, captures.get(1), "lightness")?;
    parse_capture(key, value, captures.get(2), "chroma")?;
    let hue = parse_capture(key, value, captures.get(3), "hue")?;

    if !(0.0..=1.0).contains(&lightness) {
        return Err(AppError::Validation(format!(
            "{key} lightness must be between 0 and 1"
        )));
    }

    if !(0.0..=360.0).contains(&hue) {
        return Err(AppError::Validation(format!(
            "{key} hue must be between 0 and 360"
        )));
    }

    if let Some(alpha_match) = captures.get(4) {
        let alpha = parse_capture(key, value, Some(alpha_match), "alpha")?;
        let has_percent = captures.get(5).is_some();
        let valid_alpha = if has_percent {
            (0.0..=100.0).contains(&alpha)
        } else {
            (0.0..=1.0).contains(&alpha)
        };

        if !valid_alpha {
            let range = if has_percent { "0 and 100%" } else { "0 and 1" };
            return Err(AppError::Validation(format!(
                "{key} alpha must be between {range}"
            )));
        }
    }

    Ok(())
}

fn parse_capture(
    key: &str,
    value: &str,
    capture: Option<regex::Match<'_>>,
    component: &str,
) -> Result<f64, AppError> {
    let Some(capture) = capture else {
        return Err(AppError::Validation(format!(
            "{key} has invalid {component} in {value}"
        )));
    };

    capture
        .as_str()
        .parse::<f64>()
        .map_err(|_| AppError::Validation(format!("{key} has invalid {component} in {value}")))
}

#[cfg(test)]
mod tests {
    use crate::error::AppError;

    use super::{REQUIRED_VARS, VarMap, validate_var_map};

    fn valid_map() -> VarMap {
        REQUIRED_VARS
            .iter()
            .map(|key| ((*key).to_string(), "oklch(0.5 0.1 180)".to_string()))
            .collect()
    }

    fn validation_message(result: Result<(), AppError>) -> String {
        match result {
            Err(AppError::Validation(message)) => message,
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn accepts_valid_full_map() {
        let map = valid_map();

        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn accepts_alpha_number() {
        let mut map = valid_map();
        map.insert(
            "background".to_string(),
            "oklch(0.5 0.1 180 / 0.75)".to_string(),
        );

        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn accepts_alpha_percent() {
        let mut map = valid_map();
        map.insert(
            "background".to_string(),
            "oklch(0.5 0.1 180 / 75%)".to_string(),
        );

        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn rejects_missing_var() {
        let mut map = valid_map();
        map.remove("background");

        let message = validation_message(validate_var_map(&map));

        assert!(message.contains("missing variable: background"));
    }

    #[test]
    fn rejects_unknown_var() {
        let mut map = valid_map();
        map.insert("unknown".to_string(), "oklch(0.5 0.1 180)".to_string());

        let message = validation_message(validate_var_map(&map));

        assert!(message.contains("unknown variable: unknown"));
    }

    #[test]
    fn rejects_l_out_of_range() {
        let mut map = valid_map();
        map.insert("background".to_string(), "oklch(1.1 0.1 180)".to_string());

        let message = validation_message(validate_var_map(&map));

        assert!(message.contains("background"));
        assert!(message.contains("lightness"));
    }

    #[test]
    fn rejects_h_out_of_range() {
        let mut map = valid_map();
        map.insert("background".to_string(), "oklch(0.5 0.1 361)".to_string());

        let message = validation_message(validate_var_map(&map));

        assert!(message.contains("background"));
        assert!(message.contains("hue"));
    }

    #[test]
    fn allows_high_chroma() {
        let mut map = valid_map();
        map.insert("background".to_string(), "oklch(0.5 999 180)".to_string());

        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn rejects_alpha_over_one() {
        let mut map = valid_map();
        map.insert(
            "background".to_string(),
            "oklch(0.5 0.1 180 / 1.1)".to_string(),
        );

        let message = validation_message(validate_var_map(&map));

        assert!(message.contains("background"));
        assert!(message.contains("alpha"));
    }

    #[test]
    fn rejects_alpha_percent_over_hundred() {
        let mut map = valid_map();
        map.insert(
            "background".to_string(),
            "oklch(0.5 0.1 180 / 101%)".to_string(),
        );

        let message = validation_message(validate_var_map(&map));

        assert!(message.contains("background"));
        assert!(message.contains("alpha"));
    }
}
