use super::Collector;

#[test]
fn test_system_info_populated() {
    let collector = Collector::new(true, false);
    let info = collector.system_info();
    assert!(!info.cpu_name.is_empty());
    assert!(!info.os.is_empty());
    assert!(info.cpu_cores > 0);
    assert!(info.mem_total > 0);
    assert!(info.disk_total > 0);
}

#[test]
fn test_collect_returns_valid_report() {
    let mut collector = Collector::new(true, false);
    let _ = collector.collect();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let report = collector.collect();
    assert!(report.cpu >= 0.0 && report.cpu <= 100.0);
    assert!(report.process_count > 0);
}

#[test]
fn test_cpu_usage_range() {
    let mut collector = Collector::new(true, false);
    let _ = collector.collect();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let report = collector.collect();
    assert!(report.cpu >= 0.0);
    assert!(report.cpu <= 100.0);
}

#[test]
fn test_disk_used_le_total() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    let info = collector.system_info();
    assert!(report.disk_used <= info.disk_total);
}

#[test]
fn test_memory_used_le_total() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    let info = collector.system_info();
    assert!(report.mem_used <= info.mem_total);
}
