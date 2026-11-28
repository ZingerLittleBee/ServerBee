use std::collections::HashMap;
use std::time::{Duration, Instant};

#[cfg(any(test, target_os = "linux"))]
use std::collections::HashSet;

use serverbee_common::types::DiskIo;

const DEVICE_LIST_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[cfg(target_os = "linux")]
const SECTOR_SIZE_BYTES: u64 = 512;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiskCounters {
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

pub(crate) struct DiskIoState {
    previous: HashMap<String, DiskCounters>,
    #[cfg(target_os = "linux")]
    physical_devices: Option<HashSet<String>>,
    #[cfg(not(target_os = "linux"))]
    disks: sysinfo::Disks,
    last_devices_refresh: Option<Instant>,
}

impl DiskIoState {
    pub(crate) fn new() -> Self {
        Self {
            previous: HashMap::new(),
            #[cfg(target_os = "linux")]
            physical_devices: None,
            #[cfg(not(target_os = "linux"))]
            disks: sysinfo::Disks::new_with_refreshed_list_specifics(
                sysinfo::DiskRefreshKind::nothing().with_io_usage(),
            ),
            last_devices_refresh: Some(Instant::now()),
        }
    }

    pub(crate) fn collect(&mut self, elapsed_secs: f64) -> Option<Vec<DiskIo>> {
        let current = self.read_disk_counters()?;

        if self.previous.is_empty() {
            self.previous = current;
            return Some(vec![]);
        }

        let rates = compute_disk_io(&self.previous, &current, elapsed_secs);
        self.previous = current;
        Some(rates)
    }

    fn read_disk_counters(&mut self) -> Option<HashMap<String, DiskCounters>> {
        #[cfg(target_os = "linux")]
        {
            let should_refresh = self.physical_devices.is_none()
                || self
                    .last_devices_refresh
                    .is_none_or(|t| t.elapsed() >= DEVICE_LIST_REFRESH_INTERVAL);
            if should_refresh {
                self.physical_devices = read_linux_physical_devices();
                self.last_devices_refresh = Some(Instant::now());
            }
            let physical_devices = self.physical_devices.as_ref()?;
            let diskstats = std::fs::read_to_string("/proc/diskstats").ok()?;
            Some(parse_linux_diskstats(&diskstats, physical_devices))
        }

        #[cfg(not(target_os = "linux"))]
        {
            use sysinfo::DiskRefreshKind;

            let kind = DiskRefreshKind::nothing().with_io_usage();
            let should_refresh_list = self
                .last_devices_refresh
                .is_none_or(|t| t.elapsed() >= DEVICE_LIST_REFRESH_INTERVAL);
            if should_refresh_list {
                self.disks.refresh_specifics(true, kind);
                self.last_devices_refresh = Some(Instant::now());
            } else {
                for disk in self.disks.list_mut() {
                    disk.refresh_specifics(kind);
                }
            }

            let mut counters = HashMap::with_capacity(self.disks.list().len());
            for disk in self.disks.list() {
                let name = disk.mount_point().to_string_lossy().to_string();
                if counters.contains_key(&name) {
                    continue;
                }
                let usage = disk.usage();
                counters.insert(
                    name,
                    DiskCounters {
                        read_bytes: usage.total_read_bytes,
                        write_bytes: usage.total_written_bytes,
                    },
                );
            }

            Some(counters)
        }
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
fn parse_linux_diskstats(
    content: &str,
    physical_devices: &HashSet<String>,
) -> HashMap<String, DiskCounters> {
    let mut counters = HashMap::with_capacity(physical_devices.len());

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.nth(2) else {
            continue;
        };
        if !should_track_device(name, physical_devices) {
            continue;
        }

        let Some(read_sectors) = parts.nth(2).and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        let Some(write_sectors) = parts.nth(3).and_then(|s| s.parse::<u64>().ok()) else {
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
                    .saturating_sub(previous_counters.read_bytes)
                    as f64)
                    / elapsed_secs) as u64,
                write_bytes_per_sec: ((current_counters
                    .write_bytes
                    .saturating_sub(previous_counters.write_bytes)
                    as f64)
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

    use super::{DiskCounters, compute_disk_io, should_track_device};
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

    #[test]
    fn test_compute_disk_io_with_mount_path_keys() {
        let previous = HashMap::from([
            (
                "/".to_string(),
                DiskCounters {
                    read_bytes: 1_000_000,
                    write_bytes: 2_000_000,
                },
            ),
            (
                "/home".to_string(),
                DiskCounters {
                    read_bytes: 500_000,
                    write_bytes: 300_000,
                },
            ),
        ]);
        let current = HashMap::from([
            (
                "/".to_string(),
                DiskCounters {
                    read_bytes: 1_100_000,
                    write_bytes: 2_200_000,
                },
            ),
            (
                "/home".to_string(),
                DiskCounters {
                    read_bytes: 600_000,
                    write_bytes: 400_000,
                },
            ),
        ]);

        let result = compute_disk_io(&previous, &current, 10.0);

        assert_eq!(
            result,
            vec![
                DiskIo {
                    name: "/".to_string(),
                    read_bytes_per_sec: 10_000,
                    write_bytes_per_sec: 20_000,
                },
                DiskIo {
                    name: "/home".to_string(),
                    read_bytes_per_sec: 10_000,
                    write_bytes_per_sec: 10_000,
                },
            ]
        );
    }
}
