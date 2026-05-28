use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetSizing {
    #[serde(rename = "defaultW")]
    pub default_w: u32,
    #[serde(rename = "defaultH")]
    pub default_h: u32,
    #[serde(rename = "minW")]
    pub min_w: u32,
    #[serde(rename = "minH")]
    pub min_h: u32,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "maxW")]
    pub max_w: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "maxH")]
    pub max_h: Option<u32>,
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetManifest {
    pub id: String,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    pub category: String,
    pub sizing: WidgetSizing,
    #[serde(default, rename = "requiredCaps")]
    pub required_caps: Option<Vec<String>>,
    #[serde(rename = "sdkVersion")]
    pub sdk_version: String,
}

static JSDOC_RE: Lazy<Regex> = Lazy::new(|| {
    // Match @serverbee-widget followed by JSON object until JSDoc end (*/]
    // Capture from opening { to } before the comment end
    Regex::new(r"(?s)/\*\*[\s\S]*?@serverbee-widget\s+(\{[\s\S]*?\})\s*\*/").unwrap()
});
static LINE_DECOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*\*\s?").unwrap());
static SEMVER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+\.\d+\.\d+(-[\w.]+)?$").unwrap());
static SEMVER_RANGE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[\^~]?\d+\.\d+\.\d+").unwrap());

pub fn extract_manifest(source: &str) -> Result<WidgetManifest, super::WidgetModuleError> {
    use super::WidgetModuleError as E;
    let captures = JSDOC_RE
        .captures(source)
        .ok_or_else(|| E::ManifestExtraction("no @serverbee-widget JSDoc block found".into()))?;
    let raw_json = captures.get(1).unwrap().as_str();
    let cleaned = raw_json
        .lines()
        .map(|line| LINE_DECOR_RE.replace(line, "").to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let manifest: WidgetManifest = serde_json::from_str(&cleaned)
        .map_err(|e| E::ManifestExtraction(format!("invalid JSON: {e}")))?;

    if manifest.id.is_empty() {
        return Err(E::ManifestValidation("id required".into()));
    }
    if !SEMVER_RE.is_match(&manifest.version) {
        return Err(E::ManifestValidation("version must be semver".into()));
    }
    if manifest.name.is_empty() {
        return Err(E::ManifestValidation("name required".into()));
    }
    if !matches!(
        manifest.category.as_str(),
        "Real-time" | "Charts" | "Status"
    ) {
        return Err(E::ManifestValidation("category invalid".into()));
    }
    if !matches!(
        manifest.sizing.strategy.as_str(),
        "fixed" | "free" | "aspect-square" | "content-height"
    ) {
        return Err(E::ManifestValidation("sizing.strategy invalid".into()));
    }
    if !SEMVER_RANGE_RE.is_match(&manifest.sdk_version) {
        return Err(E::ManifestValidation(
            "sdkVersion must be semver range".into(),
        ));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"/**
 * @serverbee-widget {
 *   "id": "com.example.cpu",
 *   "version": "1.0.0",
 *   "name": "CPU",
 *   "category": "Real-time",
 *   "sizing": { "defaultW": 3, "defaultH": 3, "minW": 2, "minH": 2, "strategy": "aspect-square" },
 *   "sdkVersion": "^0.1.0"
 * }
 */
export default {};
"#;

    #[test]
    fn extracts_a_valid_manifest() {
        let m = extract_manifest(GOOD).unwrap();
        assert_eq!(m.id, "com.example.cpu");
        assert_eq!(m.sizing.strategy, "aspect-square");
    }

    #[test]
    fn rejects_missing_block() {
        let res = extract_manifest("export default {};");
        assert!(matches!(
            res.unwrap_err(),
            super::super::WidgetModuleError::ManifestExtraction(_)
        ));
    }

    #[test]
    fn rejects_invalid_category() {
        let src = GOOD.replace(r#""category": "Real-time""#, r#""category": "Bogus""#);
        let res = extract_manifest(&src);
        assert!(res.is_err());
    }

    #[test]
    fn rejects_invalid_semver() {
        let src = GOOD.replace(r#""version": "1.0.0""#, r#""version": "not-semver""#);
        assert!(extract_manifest(&src).is_err());
    }
}
