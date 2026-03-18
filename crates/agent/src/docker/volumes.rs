use bollard::Docker;
use serverbee_common::docker_types::DockerVolume;

/// List all Docker volumes.
pub async fn list_volumes(docker: &Docker) -> anyhow::Result<Vec<DockerVolume>> {
    let response = docker.list_volumes::<String>(None).await?;

    let volumes = response
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|v| DockerVolume {
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
            created_at: v.created_at,
            labels: v.labels,
        })
        .collect();

    Ok(volumes)
}
