use std::sync::LazyLock;
use serde::Deserialize;

static TOML_CONTENT: &str = include_str!("targets.toml");

static PRESETS: LazyLock<Vec<FlatPresetTarget>> = LazyLock::new(|| {
    let file: PresetsFile = toml::from_str(TOML_CONTENT)
        .expect("Failed to parse presets/targets.toml");

    let mut targets = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    let valid_types = ["tcp", "icmp", "http"];

    for group in &file.presets {
        for target in &group.targets {
            assert!(!target.id.is_empty(), "Preset target ID must not be empty");
            assert!(!target.name.is_empty(), "Preset target name must not be empty");
            assert!(!target.target.is_empty(), "Preset target address must not be empty");
            assert!(
                valid_types.contains(&target.probe_type.as_str()),
                "Invalid probe_type '{}' for preset target '{}'",
                target.probe_type, target.id
            );
            assert!(
                seen_ids.insert(target.id.clone()),
                "Duplicate preset target ID: '{}'", target.id
            );

            targets.push(FlatPresetTarget {
                id: target.id.clone(),
                name: target.name.clone(),
                provider: target.provider.clone(),
                location: target.location.clone(),
                target: target.target.clone(),
                probe_type: target.probe_type.clone(),
                group_id: group.id.clone(),
                group_name: group.name.clone(),
            });
        }
    }

    targets
});

#[derive(Deserialize)]
struct PresetsFile {
    presets: Vec<PresetGroup>,
}

#[derive(Deserialize)]
struct PresetGroup {
    id: String,
    name: String,
    #[allow(dead_code)]
    description: String,
    targets: Vec<PresetTarget>,
}

#[derive(Deserialize)]
struct PresetTarget {
    id: String,
    name: String,
    provider: String,
    location: String,
    target: String,
    probe_type: String,
}

/// Flattened preset target with group metadata, used at runtime.
#[derive(Debug, Clone)]
pub struct FlatPresetTarget {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
    pub group_id: String,
    pub group_name: String,
}

pub struct PresetTargets;

impl PresetTargets {
    /// Return all preset targets. Cached via LazyLock.
    pub fn load() -> &'static [FlatPresetTarget] {
        &PRESETS
    }

    /// Find a single preset target by ID.
    pub fn find(id: &str) -> Option<&'static FlatPresetTarget> {
        PRESETS.iter().find(|t| t.id == id)
    }

    /// Check if an ID belongs to a preset target.
    pub fn is_preset(id: &str) -> bool {
        PRESETS.iter().any(|t| t.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_returns_96_targets() {
        let targets = PresetTargets::load();
        assert_eq!(targets.len(), 96);
    }

    #[test]
    fn test_all_ids_unique() {
        let targets = PresetTargets::load();
        let mut ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 96);
    }

    #[test]
    fn test_find_existing_target() {
        let target = PresetTargets::find("cn-bj-ct");
        assert!(target.is_some());
        let t = target.unwrap();
        assert_eq!(t.name, "Beijing Telecom");
        assert_eq!(t.group_id, "china-telecom");
        assert_eq!(t.group_name, "中国电信");
        assert_eq!(t.probe_type, "tcp");
    }

    #[test]
    fn test_find_nonexistent_returns_none() {
        assert!(PresetTargets::find("nonexistent").is_none());
    }

    #[test]
    fn test_is_preset() {
        assert!(PresetTargets::is_preset("cn-bj-ct"));
        assert!(PresetTargets::is_preset("intl-cloudflare"));
        assert!(!PresetTargets::is_preset("some-uuid-id"));
    }

    #[test]
    fn test_group_metadata_propagated() {
        let targets = PresetTargets::load();
        let telecom: Vec<_> = targets.iter().filter(|t| t.group_id == "china-telecom").collect();
        assert_eq!(telecom.len(), 31);
        assert!(telecom.iter().all(|t| t.group_name == "中国电信"));
    }

    #[test]
    fn test_probe_types_valid() {
        let targets = PresetTargets::load();
        let valid_types = ["tcp", "icmp", "http"];
        assert!(targets.iter().all(|t| valid_types.contains(&t.probe_type.as_str())));
    }

    #[test]
    fn test_international_targets() {
        let intl: Vec<_> = PresetTargets::load().iter()
            .filter(|t| t.group_id == "international")
            .collect();
        assert_eq!(intl.len(), 3);
        assert!(intl.iter().all(|t| t.probe_type == "icmp"));
    }
}
