use sysinfo::Disks;

/// On macOS, APFS exposes the same disk (e.g. "Macintosh HD") at both
/// `/` and `/System/Volumes/Data`, causing double-counting.
/// Deduplicate by name only on macOS; other platforms don't have this issue
/// and using name-based dedup could merge distinct volumes with the same label.
fn collect_disks() -> Vec<(u64, u64)> {
    let disks = Disks::new_with_refreshed_list();

    if cfg!(target_os = "macos") {
        let mut seen = std::collections::HashSet::new();
        disks
            .iter()
            .filter(|d| seen.insert(d.name().to_string_lossy().to_string()))
            .map(|d| (d.total_space(), d.total_space() - d.available_space()))
            .collect()
    } else {
        disks
            .iter()
            .map(|d| (d.total_space(), d.total_space() - d.available_space()))
            .collect()
    }
}

pub fn used() -> i64 {
    collect_disks().iter().map(|(_, u)| *u as i64).sum()
}

pub fn total() -> i64 {
    collect_disks().iter().map(|(t, _)| *t as i64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_disks_used_le_total_per_entry() {
        // Each entry is (total_space, used_space); used can never exceed total.
        for (total, used) in collect_disks() {
            assert!(used <= total, "used {used} must not exceed total {total}");
        }
    }

    #[test]
    fn test_used_le_total_aggregate() {
        let used = used();
        let total = total();
        assert!(used >= 0);
        assert!(total >= 0);
        assert!(used <= total, "aggregate used {used} must not exceed total {total}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_macos_dedup_unique_totals_match_collect_disks() {
        // On macos collect_disks dedups by disk name; calling it twice must
        // be deterministic and total() must equal the sum of per-entry totals.
        let entries = collect_disks();
        let summed: i64 = entries.iter().map(|(t, _)| *t as i64).sum();
        assert_eq!(summed, total());
    }
}
