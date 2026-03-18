use std::collections::HashMap;

use bollard::container::{ListContainersOptions, MemoryStatsStats, Stats, StatsOptions};
use bollard::Docker;
use futures_util::StreamExt;
use serverbee_common::docker_types::{DockerContainer, DockerContainerStats, DockerPort};

/// List all containers (running and stopped).
pub async fn list_containers(docker: &Docker) -> anyhow::Result<Vec<DockerContainer>> {
    let mut filters = HashMap::new();
    filters.insert("status", vec!["created", "restarting", "running", "removing", "paused", "exited", "dead"]);

    let options = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;

    let result: Vec<DockerContainer> = containers
        .into_iter()
        .map(|c| {
            let name = c
                .names
                .as_ref()
                .and_then(|names| names.first())
                .map(|n| n.trim_start_matches('/').to_string())
                .unwrap_or_default();

            let ports = c
                .ports
                .unwrap_or_default()
                .into_iter()
                .map(|p| DockerPort {
                    private_port: p.private_port,
                    public_port: p.public_port,
                    port_type: p.typ.map(|t| format!("{t:?}").to_lowercase()).unwrap_or_else(|| "tcp".into()),
                    ip: p.ip,
                })
                .collect();

            let labels = c.labels.unwrap_or_default();

            DockerContainer {
                id: c.id.unwrap_or_default(),
                name,
                image: c.image.unwrap_or_default(),
                state: c.state.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                created: c.created.unwrap_or(0),
                ports,
                labels,
            }
        })
        .collect();

    Ok(result)
}

/// Get stats for a set of containers (single snapshot per container).
pub async fn get_container_stats(
    docker: &Docker,
    container_ids: &[String],
) -> Vec<DockerContainerStats> {
    let mut results = Vec::with_capacity(container_ids.len());

    for id in container_ids {
        match get_single_container_stats(docker, id).await {
            Ok(stats) => results.push(stats),
            Err(e) => {
                tracing::debug!("Failed to get stats for container {id}: {e}");
            }
        }
    }

    results
}

async fn get_single_container_stats(
    docker: &Docker,
    container_id: &str,
) -> anyhow::Result<DockerContainerStats> {
    let options = StatsOptions {
        stream: false,
        one_shot: true,
    };

    let mut stream = docker.stats(container_id, Some(options));

    let stats = stream
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("No stats returned for {container_id}"))??;

    Ok(build_container_stats(container_id, &stats))
}

fn build_container_stats(container_id: &str, stats: &Stats) -> DockerContainerStats {
    let cpu_percent = calculate_cpu_percent(stats);
    let (memory_usage, memory_limit, memory_percent) = get_memory_stats(stats);
    let (network_rx, network_tx) = get_network_stats(stats);
    let (block_read, block_write) = get_block_io_stats(stats);

    let name = stats
        .name
        .trim_start_matches('/')
        .to_string();

    DockerContainerStats {
        id: container_id.to_string(),
        name,
        cpu_percent,
        memory_usage,
        memory_limit,
        memory_percent,
        network_rx,
        network_tx,
        block_read,
        block_write,
    }
}

fn calculate_cpu_percent(stats: &Stats) -> f64 {
    let cpu_stats = &stats.cpu_stats;
    let precpu_stats = &stats.precpu_stats;

    let cpu_delta = cpu_stats.cpu_usage.total_usage as f64
        - precpu_stats.cpu_usage.total_usage as f64;

    let system_delta = cpu_stats.system_cpu_usage.unwrap_or(0) as f64
        - precpu_stats.system_cpu_usage.unwrap_or(0) as f64;

    if system_delta > 0.0 && cpu_delta >= 0.0 {
        let online_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;
        (cpu_delta / system_delta) * online_cpus * 100.0
    } else {
        0.0
    }
}

fn get_memory_stats(stats: &Stats) -> (u64, u64, f64) {
    let memory_stats = &stats.memory_stats;
    let usage = memory_stats.usage.unwrap_or(0);
    let limit = memory_stats.limit.unwrap_or(0);

    // Subtract cache from usage for more accurate reading
    let cache = memory_stats
        .stats
        .as_ref()
        .map(|s| match s {
            // cgroup v2: use inactive_file
            MemoryStatsStats::V2(v2) => v2.inactive_file,
            // cgroup v1: use cache
            MemoryStatsStats::V1(v1) => v1.cache,
        })
        .unwrap_or(0);

    let actual_usage = usage.saturating_sub(cache);

    let percent = if limit > 0 {
        (actual_usage as f64 / limit as f64) * 100.0
    } else {
        0.0
    };

    (actual_usage, limit, percent)
}

fn get_network_stats(stats: &Stats) -> (u64, u64) {
    let Some(networks) = &stats.networks else {
        return (0, 0);
    };

    let mut rx: u64 = 0;
    let mut tx: u64 = 0;

    for net in networks.values() {
        rx = rx.saturating_add(net.rx_bytes);
        tx = tx.saturating_add(net.tx_bytes);
    }

    (rx, tx)
}

fn get_block_io_stats(stats: &Stats) -> (u64, u64) {
    let blkio = &stats.blkio_stats;
    let Some(ref io_service_bytes) = blkio.io_service_bytes_recursive else {
        return (0, 0);
    };

    let mut read: u64 = 0;
    let mut write: u64 = 0;

    for entry in io_service_bytes {
        match entry.op.as_str() {
            "read" | "Read" => read = read.saturating_add(entry.value),
            "write" | "Write" => write = write.saturating_add(entry.value),
            _ => {}
        }
    }

    (read, write)
}
