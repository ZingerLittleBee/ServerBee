use bollard::Docker;
use bollard::models::Volume;
use serverbee_common::docker_types::DockerVolume;

/// Pure mapping from a bollard `Volume` model to our `DockerVolume` DTO.
/// Extracted so the transformation can be unit-tested without a live daemon.
fn map_volume(v: Volume) -> DockerVolume {
    DockerVolume {
        name: v.name,
        driver: v.driver,
        mountpoint: v.mountpoint,
        created_at: v.created_at,
        labels: v.labels,
    }
}

/// List all Docker volumes.
pub async fn list_volumes(docker: &Docker) -> anyhow::Result<Vec<DockerVolume>> {
    let response = docker.list_volumes::<String>(None).await?;
    Ok(response
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(map_volume)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn map_volume_copies_all_fields() {
        let mut labels = HashMap::new();
        labels.insert("env".to_string(), "prod".to_string());
        let v = Volume {
            name: "data".to_string(),
            driver: "local".to_string(),
            mountpoint: "/var/lib/docker/volumes/data/_data".to_string(),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            labels: labels.clone(),
            ..Default::default()
        };
        let mapped = map_volume(v);
        assert_eq!(mapped.name, "data");
        assert_eq!(mapped.driver, "local");
        assert_eq!(mapped.mountpoint, "/var/lib/docker/volumes/data/_data");
        assert_eq!(mapped.created_at.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(mapped.labels.get("env"), Some(&"prod".to_string()));
    }

    #[test]
    fn map_volume_handles_missing_created_at_and_empty_labels() {
        let v = Volume {
            name: "vol".to_string(),
            driver: "local".to_string(),
            mountpoint: "/mnt".to_string(),
            created_at: None,
            labels: HashMap::new(),
            ..Default::default()
        };
        let mapped = map_volume(v);
        assert!(mapped.created_at.is_none());
        assert!(mapped.labels.is_empty());
    }

    #[test]
    fn map_volume_preserves_custom_non_local_driver() {
        // A non-`local` driver (e.g. an NFS plugin) must be carried through verbatim.
        let v = Volume {
            name: "nfs-vol".to_string(),
            driver: "nfs".to_string(),
            mountpoint: "/srv/nfs/share".to_string(),
            created_at: Some("2026-06-25T12:00:00Z".to_string()),
            labels: HashMap::new(),
            ..Default::default()
        };
        let mapped = map_volume(v);
        assert_eq!(mapped.driver, "nfs");
        assert_eq!(mapped.mountpoint, "/srv/nfs/share");
    }

    #[test]
    fn map_volume_preserves_multiple_labels_exactly() {
        // All label key/value pairs must survive the mapping unchanged.
        let mut labels = HashMap::new();
        labels.insert("com.docker.compose.project".to_string(), "app".to_string());
        labels.insert("com.docker.compose.volume".to_string(), "db".to_string());
        labels.insert("team".to_string(), "platform".to_string());
        let v = Volume {
            name: "app_db".to_string(),
            driver: "local".to_string(),
            mountpoint: "/var/lib/docker/volumes/app_db/_data".to_string(),
            created_at: None,
            labels: labels.clone(),
            ..Default::default()
        };
        let mapped = map_volume(v);
        assert_eq!(mapped.labels.len(), 3);
        assert_eq!(mapped.labels.get("com.docker.compose.project"), Some(&"app".to_string()));
        assert_eq!(mapped.labels.get("com.docker.compose.volume"), Some(&"db".to_string()));
        assert_eq!(mapped.labels.get("team"), Some(&"platform".to_string()));
    }

    #[test]
    fn map_volume_handles_empty_string_fields() {
        // Empty driver/name/mountpoint strings are passed through, not normalized away.
        let v = Volume {
            name: String::new(),
            driver: String::new(),
            mountpoint: String::new(),
            created_at: Some(String::new()),
            labels: HashMap::new(),
            ..Default::default()
        };
        let mapped = map_volume(v);
        assert_eq!(mapped.name, "");
        assert_eq!(mapped.driver, "");
        assert_eq!(mapped.mountpoint, "");
        // An empty-string timestamp stays `Some("")`, distinct from `None`.
        assert_eq!(mapped.created_at.as_deref(), Some(""));
    }

    #[test]
    fn map_volume_ignores_extra_bollard_fields() {
        // Fields outside the DTO (status/scope/options/usage_data) are dropped, only DTO fields remain.
        let mut options = HashMap::new();
        options.insert("type".to_string(), "tmpfs".to_string());
        let v = Volume {
            name: "scratch".to_string(),
            driver: "local".to_string(),
            mountpoint: "/tmp/scratch".to_string(),
            created_at: None,
            labels: HashMap::new(),
            options,
            ..Default::default()
        };
        let mapped = map_volume(v);
        // Only the five DTO fields are produced; `options` has no corresponding target.
        assert_eq!(mapped.name, "scratch");
        assert_eq!(mapped.driver, "local");
        assert!(mapped.labels.is_empty());
        assert!(mapped.created_at.is_none());
    }
}
