use std::collections::HashMap;

use bollard::Docker;
use bollard::container::{ListContainersOptions, MemoryStatsStats, Stats, StatsOptions};
use bollard::models::ContainerSummary;
use futures_util::StreamExt;
use serverbee_common::docker_types::{DockerContainer, DockerContainerStats, DockerPort};

/// Pure mapping from a bollard `ContainerSummary` to our `DockerContainer` DTO.
/// Extracted so the transformation can be unit-tested without a live daemon:
/// it normalizes the leading-slash name, port type casing, and optional fields.
fn map_container(c: ContainerSummary) -> DockerContainer {
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
            port_type: p
                .typ
                .map(|t| format!("{t:?}").to_lowercase())
                .unwrap_or_else(|| "tcp".into()),
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
}

/// List all containers (running and stopped).
pub async fn list_containers(docker: &Docker) -> anyhow::Result<Vec<DockerContainer>> {
    let mut filters = HashMap::new();
    filters.insert(
        "status",
        vec![
            "created",
            "restarting",
            "running",
            "removing",
            "paused",
            "exited",
            "dead",
        ],
    );

    let options = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(options)).await?;

    let result: Vec<DockerContainer> = containers.into_iter().map(map_container).collect();

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

    let name = stats.name.trim_start_matches('/').to_string();

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

    let cpu_delta =
        cpu_stats.cpu_usage.total_usage as f64 - precpu_stats.cpu_usage.total_usage as f64;

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

#[cfg(test)]
mod tests {
    use super::*;

    // The `Stats` struct (and its sub-structs) do not derive `Default` and
    // their `read`/`preread` field types depend on bollard feature flags
    // (chrono/time/String). To stay robust we build `Stats` via JSON
    // deserialization, supplying every required field. The default bollard
    // feature set used by this workspace makes `read`/`preread` plain strings.

    /// Build a fully-populated `Stats` value from JSON for use in tests.
    /// `extra` lets individual tests override the default skeleton.
    fn stats_from_json(value: serde_json::Value) -> Stats {
        serde_json::from_value(value).expect("valid Stats JSON")
    }

    /// A minimal-but-complete JSON skeleton that deserializes into `Stats`.
    /// Every required (non-Option, non-defaulted) field is present.
    fn base_stats_json() -> serde_json::Value {
        serde_json::json!({
            "read": "2026-03-18T10:00:00.000000000Z",
            "preread": "2026-03-18T09:59:59.000000000Z",
            "num_procs": 0,
            "pids_stats": {},
            "network": null,
            "networks": null,
            "memory_stats": {},
            "blkio_stats": {
                "io_service_bytes_recursive": null,
                "io_serviced_recursive": null,
                "io_queue_recursive": null,
                "io_service_time_recursive": null,
                "io_wait_time_recursive": null,
                "io_merged_recursive": null,
                "io_time_recursive": null,
                "sectors_recursive": null
            },
            "cpu_stats": {
                "cpu_usage": {
                    "percpu_usage": null,
                    "usage_in_usermode": 0,
                    "total_usage": 0,
                    "usage_in_kernelmode": 0
                },
                "system_cpu_usage": null,
                "online_cpus": null,
                "throttling_data": {
                    "periods": 0,
                    "throttled_periods": 0,
                    "throttled_time": 0
                }
            },
            "precpu_stats": {
                "cpu_usage": {
                    "percpu_usage": null,
                    "usage_in_usermode": 0,
                    "total_usage": 0,
                    "usage_in_kernelmode": 0
                },
                "system_cpu_usage": null,
                "online_cpus": null,
                "throttling_data": {
                    "periods": 0,
                    "throttled_periods": 0,
                    "throttled_time": 0
                }
            },
            "storage_stats": {},
            "name": "/skeleton",
            "id": "skeletonid"
        })
    }

    #[test]
    fn test_calculate_cpu_percent_normal() {
        // cpu_delta = 200 - 100 = 100, system_delta = 2000 - 1000 = 1000,
        // online_cpus = 4 -> (100/1000) * 4 * 100 = 40%
        let mut json = base_stats_json();
        json["cpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(200);
        json["cpu_stats"]["system_cpu_usage"] = serde_json::json!(2000);
        json["cpu_stats"]["online_cpus"] = serde_json::json!(4);
        json["precpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(100);
        json["precpu_stats"]["system_cpu_usage"] = serde_json::json!(1000);
        let stats = stats_from_json(json);

        let pct = calculate_cpu_percent(&stats);
        assert!(
            (pct - 40.0).abs() < f64::EPSILON,
            "expected 40% cpu, got {pct}"
        );
    }

    #[test]
    fn test_calculate_cpu_percent_default_online_cpus() {
        // online_cpus absent -> defaults to 1.
        // cpu_delta = 50, system_delta = 100 -> (50/100) * 1 * 100 = 50%
        let mut json = base_stats_json();
        json["cpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(50);
        json["cpu_stats"]["system_cpu_usage"] = serde_json::json!(100);
        json["precpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(0);
        json["precpu_stats"]["system_cpu_usage"] = serde_json::json!(0);
        let stats = stats_from_json(json);

        let pct = calculate_cpu_percent(&stats);
        assert!(
            (pct - 50.0).abs() < f64::EPSILON,
            "expected 50% cpu with default 1 online cpu, got {pct}"
        );
    }

    #[test]
    fn test_calculate_cpu_percent_zero_system_delta() {
        // system_delta == 0 -> returns 0.0 branch.
        let mut json = base_stats_json();
        json["cpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(500);
        json["cpu_stats"]["system_cpu_usage"] = serde_json::json!(1000);
        json["precpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(100);
        json["precpu_stats"]["system_cpu_usage"] = serde_json::json!(1000);
        let stats = stats_from_json(json);

        let pct = calculate_cpu_percent(&stats);
        assert_eq!(pct, 0.0, "zero system delta must yield 0%");
    }

    #[test]
    fn test_calculate_cpu_percent_negative_cpu_delta() {
        // cpu_delta < 0 (precpu greater than cpu) -> returns 0.0 branch.
        let mut json = base_stats_json();
        json["cpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(100);
        json["cpu_stats"]["system_cpu_usage"] = serde_json::json!(2000);
        json["precpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(500);
        json["precpu_stats"]["system_cpu_usage"] = serde_json::json!(1000);
        let stats = stats_from_json(json);

        let pct = calculate_cpu_percent(&stats);
        assert_eq!(pct, 0.0, "negative cpu delta must yield 0%");
    }

    #[test]
    fn test_calculate_cpu_percent_missing_system_usage_defaults_zero() {
        // Both system_cpu_usage absent -> system_delta = 0 -> 0.0
        let json = base_stats_json();
        let stats = stats_from_json(json);
        let pct = calculate_cpu_percent(&stats);
        assert_eq!(pct, 0.0, "missing system cpu usage must yield 0%");
    }

    #[test]
    fn test_get_memory_stats_v1_cache_subtracted() {
        // cgroup v1: usage 1000, cache 200 -> actual 800; limit 2000 -> 40%
        let mut json = base_stats_json();
        json["memory_stats"]["usage"] = serde_json::json!(1000);
        json["memory_stats"]["limit"] = serde_json::json!(2000);
        json["memory_stats"]["stats"] = serde_json::json!({
            "cache": 200, "dirty": 0, "mapped_file": 0, "total_inactive_file": 0,
            "pgpgout": 0, "rss": 0, "total_mapped_file": 0, "writeback": 0,
            "unevictable": 0, "pgpgin": 0, "total_unevictable": 0, "pgmajfault": 0,
            "total_rss": 0, "total_rss_huge": 0, "total_writeback": 0,
            "total_inactive_anon": 0, "rss_huge": 0, "hierarchical_memory_limit": 0,
            "total_pgfault": 0, "total_active_file": 0, "active_anon": 0,
            "total_active_anon": 0, "total_pgpgout": 0, "total_cache": 0,
            "total_dirty": 0, "inactive_anon": 0, "active_file": 0, "pgfault": 0,
            "inactive_file": 0, "total_pgmajfault": 0, "total_pgpgin": 0
        });
        let stats = stats_from_json(json);

        let (usage, limit, pct) = get_memory_stats(&stats);
        assert_eq!(usage, 800, "v1 cache must be subtracted");
        assert_eq!(limit, 2000, "limit must pass through");
        assert!((pct - 40.0).abs() < f64::EPSILON, "expected 40%, got {pct}");
    }

    #[test]
    fn test_get_memory_stats_v2_inactive_file_subtracted() {
        // cgroup v2: usage 1000, inactive_file 100 -> actual 900; limit 1000 -> 90%
        let mut json = base_stats_json();
        json["memory_stats"]["usage"] = serde_json::json!(1000);
        json["memory_stats"]["limit"] = serde_json::json!(1000);
        json["memory_stats"]["stats"] = serde_json::json!({
            "anon": 0, "file": 0, "kernel_stack": 0, "slab": 0, "sock": 0,
            "shmem": 0, "file_mapped": 0, "file_dirty": 0, "file_writeback": 0,
            "anon_thp": 0, "inactive_anon": 0, "active_anon": 0,
            "inactive_file": 100, "active_file": 0, "unevictable": 0,
            "slab_reclaimable": 0, "slab_unreclaimable": 0, "pgfault": 0,
            "pgmajfault": 0, "workingset_refault": 0, "workingset_activate": 0,
            "workingset_nodereclaim": 0, "pgrefill": 0, "pgscan": 0, "pgsteal": 0,
            "pgactivate": 0, "pgdeactivate": 0, "pglazyfree": 0, "pglazyfreed": 0,
            "thp_fault_alloc": 0, "thp_collapse_alloc": 0
        });
        let stats = stats_from_json(json);

        let (usage, limit, pct) = get_memory_stats(&stats);
        assert_eq!(usage, 900, "v2 inactive_file must be subtracted");
        assert_eq!(limit, 1000, "limit must pass through");
        assert!((pct - 90.0).abs() < f64::EPSILON, "expected 90%, got {pct}");
    }

    #[test]
    fn test_get_memory_stats_no_stats_and_zero_limit() {
        // No granular stats (cache=0), limit 0 -> percent 0.0 branch, usage passes.
        let mut json = base_stats_json();
        json["memory_stats"]["usage"] = serde_json::json!(512);
        // limit omitted -> defaults to 0
        let stats = stats_from_json(json);

        let (usage, limit, pct) = get_memory_stats(&stats);
        assert_eq!(usage, 512, "usage with no cache stays unchanged");
        assert_eq!(limit, 0, "missing limit defaults to 0");
        assert_eq!(pct, 0.0, "zero limit must yield 0% to avoid div-by-zero");
    }

    #[test]
    fn test_get_memory_stats_cache_exceeds_usage_saturates() {
        // cache > usage -> saturating_sub keeps actual at 0, not underflow.
        let mut json = base_stats_json();
        json["memory_stats"]["usage"] = serde_json::json!(100);
        json["memory_stats"]["limit"] = serde_json::json!(1000);
        json["memory_stats"]["stats"] = serde_json::json!({
            "anon": 0, "file": 0, "kernel_stack": 0, "slab": 0, "sock": 0,
            "shmem": 0, "file_mapped": 0, "file_dirty": 0, "file_writeback": 0,
            "anon_thp": 0, "inactive_anon": 0, "active_anon": 0,
            "inactive_file": 500, "active_file": 0, "unevictable": 0,
            "slab_reclaimable": 0, "slab_unreclaimable": 0, "pgfault": 0,
            "pgmajfault": 0, "workingset_refault": 0, "workingset_activate": 0,
            "workingset_nodereclaim": 0, "pgrefill": 0, "pgscan": 0, "pgsteal": 0,
            "pgactivate": 0, "pgdeactivate": 0, "pglazyfree": 0, "pglazyfreed": 0,
            "thp_fault_alloc": 0, "thp_collapse_alloc": 0
        });
        let stats = stats_from_json(json);

        let (usage, _limit, pct) = get_memory_stats(&stats);
        assert_eq!(usage, 0, "cache larger than usage must saturate to 0");
        assert_eq!(pct, 0.0, "actual usage 0 means 0%");
    }

    #[test]
    fn test_get_network_stats_none_returns_zero() {
        // networks absent -> (0, 0).
        let json = base_stats_json();
        let stats = stats_from_json(json);
        let (rx, tx) = get_network_stats(&stats);
        assert_eq!((rx, tx), (0, 0), "missing networks must yield (0, 0)");
    }

    #[test]
    fn test_get_network_stats_sums_all_interfaces() {
        // Two interfaces summed: rx 100+50=150, tx 10+5=15.
        let net = |rx: u64, tx: u64| {
            serde_json::json!({
                "rx_dropped": 0, "rx_bytes": rx, "rx_errors": 0, "tx_packets": 0,
                "tx_dropped": 0, "rx_packets": 0, "tx_errors": 0, "tx_bytes": tx
            })
        };
        let mut json = base_stats_json();
        json["networks"] = serde_json::json!({
            "eth0": net(100, 10),
            "eth1": net(50, 5)
        });
        let stats = stats_from_json(json);

        let (rx, tx) = get_network_stats(&stats);
        assert_eq!(rx, 150, "rx must sum across interfaces");
        assert_eq!(tx, 15, "tx must sum across interfaces");
    }

    #[test]
    fn test_get_block_io_stats_none_returns_zero() {
        // io_service_bytes_recursive is null -> (0, 0).
        let json = base_stats_json();
        let stats = stats_from_json(json);
        let (read, write) = get_block_io_stats(&stats);
        assert_eq!((read, write), (0, 0), "missing blkio must yield (0, 0)");
    }

    #[test]
    fn test_get_block_io_stats_mixed_ops() {
        // Covers lower/upper case op variants and the ignored default arm.
        let entry = |op: &str, value: u64| {
            serde_json::json!({ "major": 8, "minor": 0, "op": op, "value": value })
        };
        let mut json = base_stats_json();
        json["blkio_stats"]["io_service_bytes_recursive"] = serde_json::json!([
            entry("read", 100),
            entry("Read", 50),
            entry("write", 200),
            entry("Write", 25),
            entry("sync", 999),
            entry("async", 999)
        ]);
        let stats = stats_from_json(json);

        let (read, write) = get_block_io_stats(&stats);
        assert_eq!(read, 150, "read must sum 'read' and 'Read' entries");
        assert_eq!(write, 225, "write must sum 'write' and 'Write' entries");
    }

    #[test]
    fn test_build_container_stats_full_and_name_trim() {
        // End-to-end: build_container_stats wires the helpers together and
        // strips the leading '/' from the container name.
        let net = serde_json::json!({
            "rx_dropped": 0, "rx_bytes": 300, "rx_errors": 0, "tx_packets": 0,
            "tx_dropped": 0, "rx_packets": 0, "tx_errors": 0, "tx_bytes": 30
        });
        let mut json = base_stats_json();
        json["name"] = serde_json::json!("/my-container");
        json["cpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(200);
        json["cpu_stats"]["system_cpu_usage"] = serde_json::json!(1200);
        json["cpu_stats"]["online_cpus"] = serde_json::json!(1);
        json["precpu_stats"]["cpu_usage"]["total_usage"] = serde_json::json!(100);
        json["precpu_stats"]["system_cpu_usage"] = serde_json::json!(200);
        json["memory_stats"]["usage"] = serde_json::json!(400);
        json["memory_stats"]["limit"] = serde_json::json!(800);
        json["networks"] = serde_json::json!({ "eth0": net });
        json["blkio_stats"]["io_service_bytes_recursive"] = serde_json::json!([
            { "major": 8, "minor": 0, "op": "read", "value": 11 },
            { "major": 8, "minor": 0, "op": "write", "value": 22 }
        ]);
        let stats = stats_from_json(json);

        let out = build_container_stats("container-id-abc", &stats);
        assert_eq!(out.id, "container-id-abc", "id is the passed argument");
        assert_eq!(out.name, "my-container", "leading slash must be trimmed");
        assert_eq!(out.network_rx, 300);
        assert_eq!(out.network_tx, 30);
        assert_eq!(out.block_read, 11);
        assert_eq!(out.block_write, 22);
        assert_eq!(out.memory_usage, 400);
        assert_eq!(out.memory_limit, 800);
        // cpu: delta 100 / system 1000 * 1 * 100 = 10%
        assert!(
            (out.cpu_percent - 10.0).abs() < 1e-9,
            "expected 10% cpu, got {}",
            out.cpu_percent
        );
        // mem: 400 / 800 = 50%
        assert!(
            (out.memory_percent - 50.0).abs() < f64::EPSILON,
            "expected 50% mem, got {}",
            out.memory_percent
        );
    }

    #[test]
    fn test_build_container_stats_name_without_slash() {
        // Name lacking a leading slash must be preserved verbatim.
        let mut json = base_stats_json();
        json["name"] = serde_json::json!("no-slash");
        let stats = stats_from_json(json);

        let out = build_container_stats("id1", &stats);
        assert_eq!(out.name, "no-slash", "name without slash stays unchanged");
        // Empty/default stats -> all metrics zero.
        assert_eq!(out.cpu_percent, 0.0);
        assert_eq!(out.memory_usage, 0);
        assert_eq!(out.network_rx, 0);
        assert_eq!(out.block_read, 0);
    }

    // --- map_container: pure ContainerSummary -> DockerContainer mapping ---

    use bollard::models::{Port, PortTypeEnum};

    /// A running container with a leading-slash name maps all fields through
    /// and strips the '/' from the name.
    #[test]
    fn test_map_container_running_full() {
        let mut labels = HashMap::new();
        labels.insert("com.docker.compose.project".to_string(), "web".to_string());

        let summary = ContainerSummary {
            id: Some("abc123".to_string()),
            names: Some(vec!["/nginx".to_string()]),
            image: Some("nginx:latest".to_string()),
            state: Some("running".to_string()),
            status: Some("Up 3 hours".to_string()),
            created: Some(1_700_000_000),
            labels: Some(labels.clone()),
            ports: Some(vec![Port {
                ip: Some("0.0.0.0".to_string()),
                private_port: 80,
                public_port: Some(8080),
                typ: Some(PortTypeEnum::TCP),
            }]),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(out.id, "abc123", "id passes through");
        assert_eq!(out.name, "nginx", "leading slash is stripped from name");
        assert_eq!(out.image, "nginx:latest", "image passes through");
        assert_eq!(out.state, "running", "running state passes through");
        assert_eq!(out.status, "Up 3 hours", "status passes through");
        assert_eq!(out.created, 1_700_000_000, "created timestamp passes through");
        assert_eq!(out.labels, labels, "labels pass through");
        assert_eq!(out.ports.len(), 1, "single port is mapped");
        assert_eq!(out.ports[0].private_port, 80);
        assert_eq!(out.ports[0].public_port, Some(8080));
        assert_eq!(out.ports[0].port_type, "tcp", "TCP enum lowercases to 'tcp'");
        assert_eq!(out.ports[0].ip.as_deref(), Some("0.0.0.0"));
    }

    /// An exited container maps its state/status verbatim and tolerates an
    /// internal port with no public mapping or host IP.
    #[test]
    fn test_map_container_exited_internal_port() {
        let summary = ContainerSummary {
            id: Some("dead01".to_string()),
            names: Some(vec!["/batch-job".to_string()]),
            image: Some("busybox".to_string()),
            state: Some("exited".to_string()),
            status: Some("Exited (0) 5 minutes ago".to_string()),
            created: Some(42),
            ports: Some(vec![Port {
                ip: None,
                private_port: 9000,
                public_port: None,
                typ: Some(PortTypeEnum::UDP),
            }]),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(out.state, "exited", "exited state passes through");
        assert_eq!(out.status, "Exited (0) 5 minutes ago");
        assert_eq!(out.ports[0].public_port, None, "no public port stays None");
        assert_eq!(out.ports[0].ip, None, "missing host IP stays None");
        assert_eq!(out.ports[0].port_type, "udp", "UDP enum lowercases to 'udp'");
    }

    /// A paused container maps its 'paused' state through unchanged.
    #[test]
    fn test_map_container_paused_state() {
        let summary = ContainerSummary {
            id: Some("p1".to_string()),
            names: Some(vec!["/db".to_string()]),
            state: Some("paused".to_string()),
            status: Some("Up 2 days (Paused)".to_string()),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(out.state, "paused", "paused state passes through");
        assert_eq!(out.status, "Up 2 days (Paused)");
        assert!(out.ports.is_empty(), "no ports field yields empty vec");
    }

    /// A name without a leading slash is preserved verbatim.
    #[test]
    fn test_map_container_name_without_slash() {
        let summary = ContainerSummary {
            names: Some(vec!["plain-name".to_string()]),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(out.name, "plain-name", "name without slash is untouched");
    }

    /// When several names are present only the first is used (deduplicated).
    #[test]
    fn test_map_container_uses_first_name() {
        let summary = ContainerSummary {
            names: Some(vec!["/primary".to_string(), "/alias".to_string()]),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(out.name, "primary", "only the first name is mapped");
    }

    /// All optional fields absent -> sensible defaults (empty strings, 0 created,
    /// empty ports/labels) instead of panicking.
    #[test]
    fn test_map_container_all_fields_missing_defaults() {
        let summary = ContainerSummary::default();

        let out = map_container(summary);
        assert_eq!(out.id, "", "missing id defaults to empty string");
        assert_eq!(out.name, "", "missing names defaults to empty string");
        assert_eq!(out.image, "", "missing image defaults to empty string");
        assert_eq!(out.state, "", "missing state defaults to empty string");
        assert_eq!(out.status, "", "missing status defaults to empty string");
        assert_eq!(out.created, 0, "missing created defaults to 0");
        assert!(out.ports.is_empty(), "missing ports defaults to empty vec");
        assert!(out.labels.is_empty(), "missing labels defaults to empty map");
    }

    /// An empty `names` vec falls back to the default empty name (first() is None).
    #[test]
    fn test_map_container_empty_names_vec() {
        let summary = ContainerSummary {
            names: Some(vec![]),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(out.name, "", "empty names vec yields empty name");
    }

    /// A port whose `typ` is absent falls back to the "tcp" default.
    #[test]
    fn test_map_container_port_missing_type_defaults_tcp() {
        let summary = ContainerSummary {
            names: Some(vec!["/svc".to_string()]),
            ports: Some(vec![Port {
                ip: Some("127.0.0.1".to_string()),
                private_port: 5432,
                public_port: Some(5432),
                typ: None,
            }]),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(
            out.ports[0].port_type, "tcp",
            "missing port type defaults to 'tcp'"
        );
    }

    /// Multiple ports are all mapped and keep their order.
    #[test]
    fn test_map_container_multiple_ports() {
        let summary = ContainerSummary {
            names: Some(vec!["/multi".to_string()]),
            ports: Some(vec![
                Port {
                    ip: None,
                    private_port: 80,
                    public_port: Some(80),
                    typ: Some(PortTypeEnum::TCP),
                },
                Port {
                    ip: None,
                    private_port: 443,
                    public_port: Some(443),
                    typ: Some(PortTypeEnum::SCTP),
                },
            ]),
            ..Default::default()
        };

        let out = map_container(summary);
        assert_eq!(out.ports.len(), 2, "both ports are mapped");
        assert_eq!(out.ports[0].private_port, 80);
        assert_eq!(out.ports[1].private_port, 443);
        assert_eq!(
            out.ports[1].port_type, "sctp",
            "SCTP enum lowercases to 'sctp'"
        );
    }
}
