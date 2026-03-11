use sysinfo::Disks;

pub fn used() -> i64 {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .map(|d| (d.total_space() - d.available_space()) as i64)
        .sum()
}

pub fn total() -> i64 {
    let disks = Disks::new_with_refreshed_list();
    disks.iter().map(|d| d.total_space() as i64).sum()
}
