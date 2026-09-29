use sysinfo::Disks;

/// One mounted filesystem as reported by sysinfo, reduced to what the
/// aggregate needs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiskEntry {
    /// The mount source: the device path or remote spec on Linux
    /// (`/dev/vdb`, `host:/export`), the volume name on macOS.
    source: String,
    total: u64,
    available: u64,
}

impl DiskEntry {
    fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

fn collect_disks() -> Vec<DiskEntry> {
    let disks = Disks::new_with_refreshed_list();
    let entries = disks
        .iter()
        .map(|d| DiskEntry {
            source: d.name().to_string_lossy().to_string(),
            total: d.total_space(),
            available: d.available_space(),
        })
        .collect();

    if cfg!(target_os = "macos") {
        dedup_by_source(entries)
    } else if cfg!(target_os = "linux") {
        dedup_linux_mounts(entries)
    } else {
        // Windows reports volume labels, which distinct drives can share.
        entries
    }
}

/// On macOS, APFS exposes the same disk (e.g. "Macintosh HD") at both
/// `/` and `/System/Volumes/Data`, causing double-counting.
fn dedup_by_source(entries: Vec<DiskEntry>) -> Vec<DiskEntry> {
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|e| seen.insert(e.source.clone()))
        .collect()
}

/// Linux lists every mount point, so one filesystem mounted several times
/// (bind mounts, btrfs subvolumes, the same SSHFS remote mounted twice, the
/// bind-mounted `/etc/hosts` of a container) would be summed once per mount.
///
/// Entries whose source names a device or remote (`/dev/vdb`, `//nas/share`,
/// `host:/path`) are deduplicated by that source. Generic sources such as
/// `overlay` or `none` say nothing about the backing store, so they are only
/// dropped when they report the same capacity and free space as a filesystem
/// already counted (a container's overlay root over the host disk).
fn dedup_linux_mounts(entries: Vec<DiskEntry>) -> Vec<DiskEntry> {
    let (specific, generic): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|e| is_specific_source(&e.source));

    let mut kept = dedup_by_source(specific);
    for entry in generic {
        if !kept.iter().any(|k| same_filesystem_stats(k, &entry)) {
            kept.push(entry);
        }
    }
    kept
}

fn is_specific_source(source: &str) -> bool {
    source.starts_with('/') || source.contains(':')
}

/// Same capacity, and free space within 0.1% (writes can land between the
/// two `statvfs` calls).
fn same_filesystem_stats(a: &DiskEntry, b: &DiskEntry) -> bool {
    a.total > 0 && a.total == b.total && a.available.abs_diff(b.available) <= a.total / 1000
}

pub fn used() -> i64 {
    collect_disks().iter().map(|e| e.used() as i64).sum()
}

pub fn total() -> i64 {
    collect_disks().iter().map(|e| e.total as i64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn entry(source: &str, total: u64, available: u64) -> DiskEntry {
        DiskEntry {
            source: source.to_string(),
            total,
            available,
        }
    }

    fn sum_total(entries: &[DiskEntry]) -> u64 {
        entries.iter().map(|e| e.total).sum()
    }

    fn sum_used(entries: &[DiskEntry]) -> u64 {
        entries.iter().map(DiskEntry::used).sum()
    }

    #[test]
    fn test_collect_disks_used_le_total_per_entry() {
        for e in collect_disks() {
            assert!(
                e.available <= e.total,
                "available must not exceed total: {e:?}"
            );
        }
    }

    #[test]
    fn test_used_le_total_aggregate() {
        let used = used();
        let total = total();
        assert!(used >= 0);
        assert!(total >= 0);
        assert!(
            used <= total,
            "aggregate used {used} must not exceed total {total}"
        );
    }

    /// The layout from issue #183: one data disk bind-mounted nine times and
    /// two SSHFS remotes mounted twice each.
    #[test]
    fn test_linux_bind_and_duplicate_sshfs_mounts_count_once() {
        let mut entries = vec![
            entry("/dev/vda3", 8 * GB, 3 * GB),
            entry("/dev/vda1", GB, GB / 2),
        ];
        for _ in 0..9 {
            entries.push(entry("/dev/vdb", 196 * GB, 60 * GB));
        }
        for source in ["remote-a:/data/backup", "remote-b:/data/cloud"] {
            for _ in 0..2 {
                entries.push(entry(source, 245 * GB, 100 * GB));
            }
        }

        let kept = dedup_linux_mounts(entries);

        assert_eq!(kept.len(), 5);
        assert_eq!(sum_total(&kept), (8 + 1 + 196 + 245 + 245) * GB);
        assert_eq!(sum_used(&kept), (5 + 136 + 145 + 145) * GB + GB / 2);
    }

    /// Inside a container: the overlay root and Docker's bind-mounted
    /// /etc/hosts, /etc/hostname and /etc/resolv.conf all sit on the host disk.
    #[test]
    fn test_linux_container_overlay_and_bind_files_count_once() {
        let entries = vec![
            entry("overlay", 800 * GB, 760 * GB),
            entry("/dev/vdb1", 800 * GB, 760 * GB + 4096),
            entry("/dev/vdb1", 800 * GB, 760 * GB),
            entry("/dev/vdb1", 800 * GB, 760 * GB),
        ];

        let kept = dedup_linux_mounts(entries);

        assert_eq!(kept.len(), 1);
        assert_eq!(sum_total(&kept), 800 * GB);
    }

    #[test]
    fn test_linux_distinct_devices_and_generic_sources_are_kept() {
        let entries = vec![
            entry("/dev/sda1", 500 * GB, 200 * GB),
            entry("/dev/sdb1", 500 * GB, 200 * GB),
            entry("//nas/share", 2000 * GB, 900 * GB),
            entry("overlay", 50 * GB, 20 * GB),
            entry("none", 10 * GB, 9 * GB),
        ];

        let kept = dedup_linux_mounts(entries.clone());

        assert_eq!(kept.len(), entries.len());
    }

    #[test]
    fn test_linux_generic_sources_merge_with_each_other_only_on_matching_stats() {
        let entries = vec![
            entry("overlay", 100 * GB, 40 * GB),
            entry("overlay", 100 * GB, 40 * GB),
            entry("overlay", 100 * GB, 10 * GB),
        ];

        let kept = dedup_linux_mounts(entries);

        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn test_macos_dedup_keeps_first_entry_per_volume_name() {
        let entries = vec![
            entry("Macintosh HD", 500 * GB, 100 * GB),
            entry("Macintosh HD", 500 * GB, 100 * GB),
            entry("Backup", 1000 * GB, 800 * GB),
        ];

        let kept = dedup_by_source(entries);

        assert_eq!(kept.len(), 2);
        assert_eq!(sum_total(&kept), 1500 * GB);
    }
}
