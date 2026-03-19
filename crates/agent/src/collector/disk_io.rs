use std::collections::HashMap;

#[cfg(any(test, target_os = "linux"))]
use std::collections::HashSet;

use serverbee_common::types::DiskIo;

#[cfg(target_os = "linux")]
const SECTOR_SIZE_BYTES: u64 = 512;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiskCounters {
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

pub fn collect(elapsed_secs: f64, previous: &mut HashMap<String, DiskCounters>) -> Option<Vec<DiskIo>> {
    let current = read_disk_counters()?;

    if previous.is_empty() {
        *previous = current;
        return Some(vec![]);
    }

    let rates = compute_disk_io(previous, &current, elapsed_secs);
    *previous = current;
    Some(rates)
}

fn read_disk_counters() -> Option<HashMap<String, DiskCounters>> {
    #[cfg(target_os = "linux")]
    {
        let physical_devices = read_linux_physical_devices()?;
        let diskstats = std::fs::read_to_string("/proc/diskstats").ok()?;
        Some(parse_linux_diskstats(&diskstats, &physical_devices))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn read_linux_physical_devices() -> Option<HashSet<String>> {
    let mut devices = HashSet::new();

    for entry in std::fs::read_dir("/sys/block").ok()? {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();

        if is_virtual_device(&name) {
            continue;
        }

        if !entry.path().join("device").exists() {
            continue;
        }

        devices.insert(name);
    }

    Some(devices)
}

#[cfg(target_os = "linux")]
fn parse_linux_diskstats(content: &str, physical_devices: &HashSet<String>) -> HashMap<String, DiskCounters> {
    let mut counters = HashMap::new();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() <= 9 {
            continue;
        }

        let name = parts[2];
        if !should_track_device(name, physical_devices) {
            continue;
        }

        let read_sectors = parts[5].parse::<u64>().ok();
        let write_sectors = parts[9].parse::<u64>().ok();
        let (Some(read_sectors), Some(write_sectors)) = (read_sectors, write_sectors) else {
            continue;
        };

        counters.insert(
            name.to_string(),
            DiskCounters {
                read_bytes: read_sectors.saturating_mul(SECTOR_SIZE_BYTES),
                write_bytes: write_sectors.saturating_mul(SECTOR_SIZE_BYTES),
            },
        );
    }

    counters
}

fn compute_disk_io(
    previous: &HashMap<String, DiskCounters>,
    current: &HashMap<String, DiskCounters>,
    elapsed_secs: f64,
) -> Vec<DiskIo> {
    let elapsed_secs = elapsed_secs.max(1.0);
    let mut disk_io = current
        .iter()
        .map(|(name, current_counters)| {
            let previous_counters = previous.get(name).cloned().unwrap_or_default();

            DiskIo {
                name: name.clone(),
                read_bytes_per_sec: ((current_counters
                    .read_bytes
                    .saturating_sub(previous_counters.read_bytes) as f64)
                    / elapsed_secs) as u64,
                write_bytes_per_sec: ((current_counters
                    .write_bytes
                    .saturating_sub(previous_counters.write_bytes) as f64)
                    / elapsed_secs) as u64,
            }
        })
        .collect::<Vec<_>>();

    disk_io.sort_by(|left, right| left.name.cmp(&right.name));
    disk_io
}

#[cfg(any(test, target_os = "linux"))]
fn should_track_device(name: &str, physical_devices: &HashSet<String>) -> bool {
    physical_devices.contains(name) && !is_virtual_device(name)
}

#[cfg(any(test, target_os = "linux"))]
fn is_virtual_device(name: &str) -> bool {
    name.starts_with("loop")
        || name.starts_with("dm-")
        || name.starts_with("ram")
        || name.starts_with("sr")
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::{compute_disk_io, should_track_device, DiskCounters};
    use serverbee_common::types::DiskIo;

    #[test]
    fn test_compute_disk_io_sorts_devices_and_clamps_negative_deltas() {
        let previous = HashMap::from([
            (
                "sdb".to_string(),
                DiskCounters {
                    read_bytes: 5_000,
                    write_bytes: 4_000,
                },
            ),
            (
                "sda".to_string(),
                DiskCounters {
                    read_bytes: 1_000,
                    write_bytes: 2_000,
                },
            ),
        ]);
        let current = HashMap::from([
            (
                "sdb".to_string(),
                DiskCounters {
                    read_bytes: 4_000,
                    write_bytes: 6_000,
                },
            ),
            (
                "sda".to_string(),
                DiskCounters {
                    read_bytes: 3_000,
                    write_bytes: 5_000,
                },
            ),
        ]);

        let actual = compute_disk_io(&previous, &current, 2.0);

        assert_eq!(
            actual,
            vec![
                DiskIo {
                    name: "sda".to_string(),
                    read_bytes_per_sec: 1_000,
                    write_bytes_per_sec: 1_500,
                },
                DiskIo {
                    name: "sdb".to_string(),
                    read_bytes_per_sec: 0,
                    write_bytes_per_sec: 1_000,
                },
            ]
        );
    }

    #[test]
    fn test_should_track_device_filters_virtual_and_partition_names() {
        let physical = HashSet::from([
            "sda".to_string(),
            "nvme0n1".to_string(),
            "loop0".to_string(),
        ]);

        assert!(should_track_device("sda", &physical));
        assert!(should_track_device("nvme0n1", &physical));
        assert!(!should_track_device("sda1", &physical));
        assert!(!should_track_device("nvme0n1p1", &physical));
        assert!(!should_track_device("loop0", &physical));
        assert!(!should_track_device("dm-0", &physical));
        assert!(!should_track_device("sr0", &physical));
    }
}
