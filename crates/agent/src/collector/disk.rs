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
