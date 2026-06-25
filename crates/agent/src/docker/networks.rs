use std::collections::HashMap;

use bollard::Docker;
use bollard::models::Network;
use serverbee_common::docker_types::DockerNetwork;

/// Pure mapping from a bollard `Network` model to our `DockerNetwork` DTO.
/// Extracted so the transformation can be unit-tested without a live daemon.
fn map_network(n: Network) -> DockerNetwork {
    let containers: HashMap<String, String> = n
        .containers
        .unwrap_or_default()
        .into_iter()
        .map(|(id, info)| {
            let name = info.name.unwrap_or_default();
            (id, name)
        })
        .collect();

    DockerNetwork {
        id: n.id.unwrap_or_default(),
        name: n.name.unwrap_or_default(),
        driver: n.driver.unwrap_or_default(),
        scope: n.scope.unwrap_or_default(),
        containers,
    }
}

/// List all Docker networks.
pub async fn list_networks(docker: &Docker) -> anyhow::Result<Vec<DockerNetwork>> {
    let networks = docker.list_networks::<String>(None).await?;
    Ok(networks.into_iter().map(map_network).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::NetworkContainer;

    #[test]
    fn map_network_fills_defaults_for_missing_fields() {
        // An all-None network maps to empty strings and an empty container map.
        let n = Network::default();
        let mapped = map_network(n);
        assert_eq!(mapped.id, "");
        assert_eq!(mapped.name, "");
        assert_eq!(mapped.driver, "");
        assert_eq!(mapped.scope, "");
        assert!(mapped.containers.is_empty());
    }

    #[test]
    fn map_network_maps_all_fields_and_container_names() {
        let mut containers = HashMap::new();
        containers.insert(
            "c1".to_string(),
            NetworkContainer {
                name: Some("web".to_string()),
                ..Default::default()
            },
        );
        // A container whose name is None must fall back to an empty string.
        containers.insert(
            "c2".to_string(),
            NetworkContainer {
                name: None,
                ..Default::default()
            },
        );
        let n = Network {
            id: Some("net123".to_string()),
            name: Some("bridge".to_string()),
            driver: Some("bridge".to_string()),
            scope: Some("local".to_string()),
            containers: Some(containers),
            ..Default::default()
        };
        let mapped = map_network(n);
        assert_eq!(mapped.id, "net123");
        assert_eq!(mapped.name, "bridge");
        assert_eq!(mapped.driver, "bridge");
        assert_eq!(mapped.scope, "local");
        assert_eq!(mapped.containers.get("c1"), Some(&"web".to_string()));
        assert_eq!(mapped.containers.get("c2"), Some(&String::new()));
    }
}
