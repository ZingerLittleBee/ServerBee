use std::collections::HashMap;

use bollard::Docker;
use serverbee_common::docker_types::DockerNetwork;

/// List all Docker networks.
pub async fn list_networks(docker: &Docker) -> anyhow::Result<Vec<DockerNetwork>> {
    let networks = docker.list_networks::<String>(None).await?;

    let result: Vec<DockerNetwork> = networks
        .into_iter()
        .map(|n| {
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
        })
        .collect();

    Ok(result)
}
