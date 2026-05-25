use serde::{Deserialize, Serialize};

use crate::service::spa_theme::error::SpaThemeError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub min_serverbee_version: Option<String>,
    #[serde(default)]
    pub preview: Option<String>,
}

fn default_entry() -> String {
    "index.html".into()
}

pub const SCHEMA_VERSION: u32 = 1;
pub const ID_REGEX: &str = r"^[a-z][a-z0-9-]{2,63}$";
pub const MAX_NAME: usize = 64;
pub const MAX_AUTHOR: usize = 64;
pub const MAX_DESCRIPTION: usize = 500;

impl ThemeManifest {
    /// Parse manifest JSON bytes and validate every field. `running_version`
    /// is the server's semver (used for min_serverbee_version check).
    /// `file_paths` is the set of paths present in the package (for entry/preview existence).
    pub fn parse_and_validate(
        bytes: &[u8],
        running_version: &semver::Version,
        file_paths: &std::collections::HashSet<String>,
    ) -> Result<Self, SpaThemeError> {
        let mut m: Self = serde_json::from_slice(bytes).map_err(|e| SpaThemeError::InvalidManifest {
            field: "$",
            reason: format!("JSON parse: {e}"),
        })?;

        if m.schema_version != SCHEMA_VERSION {
            return Err(SpaThemeError::InvalidManifest {
                field: "schema_version",
                reason: format!("must be {SCHEMA_VERSION}"),
            });
        }

        let re = regex::Regex::new(ID_REGEX).expect("static regex");
        if !re.is_match(&m.id) {
            return Err(SpaThemeError::InvalidManifest {
                field: "id",
                reason: format!("must match {ID_REGEX}"),
            });
        }

        let name_trim = m.name.trim();
        if name_trim.is_empty() || name_trim.chars().count() > MAX_NAME {
            return Err(SpaThemeError::InvalidManifest {
                field: "name",
                reason: format!("1..={MAX_NAME} chars"),
            });
        }
        m.name = strip_html(name_trim);

        if let Some(a) = &m.author {
            if a.chars().count() > MAX_AUTHOR {
                return Err(SpaThemeError::InvalidManifest {
                    field: "author",
                    reason: format!("..={MAX_AUTHOR} chars"),
                });
            }
            m.author = Some(strip_html(a));
        }
        if let Some(d) = &m.description {
            if d.chars().count() > MAX_DESCRIPTION {
                return Err(SpaThemeError::InvalidManifest {
                    field: "description",
                    reason: format!("..={MAX_DESCRIPTION} chars"),
                });
            }
            m.description = Some(strip_html(d));
        }
        if let Some(h) = &m.homepage {
            let u = url::Url::parse(h).map_err(|_| SpaThemeError::InvalidManifest {
                field: "homepage",
                reason: "invalid URL".into(),
            })?;
            if !matches!(u.scheme(), "http" | "https") {
                return Err(SpaThemeError::InvalidManifest {
                    field: "homepage",
                    reason: "must be http(s)".into(),
                });
            }
        }

        semver::Version::parse(&m.version).map_err(|_| SpaThemeError::InvalidManifest {
            field: "version",
            reason: "invalid semver".into(),
        })?;

        if let Some(min) = &m.min_serverbee_version {
            let parsed = semver::Version::parse(min).map_err(|_| SpaThemeError::InvalidManifest {
                field: "min_serverbee_version",
                reason: "invalid semver".into(),
            })?;
            if &parsed > running_version {
                return Err(SpaThemeError::IncompatibleVersion {
                    min: min.clone(),
                    running: running_version.to_string(),
                });
            }
        }

        if !m.entry.ends_with(".html") {
            return Err(SpaThemeError::InvalidManifest {
                field: "entry",
                reason: "must end with .html".into(),
            });
        }
        if !file_paths.contains(&m.entry) {
            return Err(SpaThemeError::MissingEntry { entry: m.entry.clone() });
        }
        if let Some(p) = &m.preview {
            if !file_paths.contains(p) {
                return Err(SpaThemeError::InvalidManifest {
                    field: "preview",
                    reason: format!("not in package: {p}"),
                });
            }
            let lower = p.to_ascii_lowercase();
            if !(lower.ends_with(".png")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".webp"))
            {
                return Err(SpaThemeError::InvalidManifest {
                    field: "preview",
                    reason: "must be png/jpg/webp".into(),
                });
            }
        }

        Ok(m)
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            other if !in_tag => out.push(other),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn files(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|s| (*s).into()).collect()
    }
    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    fn valid_manifest() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "id": "acme",
            "name": "Acme",
            "version": "1.0.0",
        })
    }

    #[test]
    fn happy_path() {
        let m = ThemeManifest::parse_and_validate(
            valid_manifest().to_string().as_bytes(),
            &v("1.0.0-alpha.3"),
            &files(&["index.html"]),
        )
        .unwrap();
        assert_eq!(m.id, "acme");
        assert_eq!(m.entry, "index.html");
    }

    #[test]
    fn rejects_bad_id() {
        let mut m = valid_manifest();
        m["id"] = serde_json::json!("Acme!");
        let err =
            ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"]))
                .unwrap_err();
        assert!(matches!(err, SpaThemeError::InvalidManifest { field: "id", .. }));
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let mut m = valid_manifest();
        m["schema_version"] = serde_json::json!(2);
        let err =
            ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"]))
                .unwrap_err();
        assert!(matches!(err, SpaThemeError::InvalidManifest { field: "schema_version", .. }));
    }

    #[test]
    fn rejects_missing_entry() {
        let m = valid_manifest();
        let err = ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&[])).unwrap_err();
        assert!(matches!(err, SpaThemeError::MissingEntry { .. }));
    }

    #[test]
    fn rejects_min_version_above_running() {
        let mut m = valid_manifest();
        m["min_serverbee_version"] = serde_json::json!("2.0.0");
        let err =
            ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0-alpha.3"), &files(&["index.html"]))
                .unwrap_err();
        assert!(matches!(err, SpaThemeError::IncompatibleVersion { .. }));
    }

    #[test]
    fn accepts_min_version_lt_alpha() {
        let mut m = valid_manifest();
        m["min_serverbee_version"] = serde_json::json!("0.9.0");
        ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0-alpha.3"), &files(&["index.html"]))
            .unwrap();
    }

    #[test]
    fn strips_html_from_name() {
        let mut m = valid_manifest();
        m["name"] = serde_json::json!("<script>alert(1)</script>Acme");
        let parsed =
            ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"]))
                .unwrap();
        assert!(!parsed.name.contains('<'));
        assert!(parsed.name.contains("Acme"));
    }

    #[test]
    fn rejects_bad_homepage() {
        let mut m = valid_manifest();
        m["homepage"] = serde_json::json!("javascript:alert(1)");
        let err =
            ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"]))
                .unwrap_err();
        assert!(matches!(err, SpaThemeError::InvalidManifest { field: "homepage", .. }));
    }
}
