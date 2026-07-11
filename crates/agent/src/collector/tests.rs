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

#[cfg(target_os = "linux")]
#[test]
fn test_collect_disk_io_first_sample_is_empty() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    assert_eq!(report.disk_io, Some(vec![]));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn test_collect_disk_io_first_sample_is_empty_on_non_linux() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    assert_eq!(report.disk_io, Some(vec![]));
}

#[test]
fn test_collect_with_gpu_enabled_does_not_panic() {
    // With `enable_gpu` true the collector invokes the real GPU probe through
    // SysinfoSource. Without the `gpu` cargo feature (the default, and the
    // case on CI hosts with no NVIDIA GPU) this returns None, but the branch
    // must be reached and must not panic. When a report is present, validate
    // its invariants.
    let mut collector = Collector::new(false, true);
    let report = collector.collect();
    if let Some(gpu) = report.gpu {
        assert_eq!(gpu.count as usize, gpu.detailed_info.len());
        assert!(gpu.average_usage.is_finite());
    }
}

#[test]
fn test_system_info_static_assembly_fields() {
    // The `system_info` assembly fields not covered by the populated-fields
    // test: arch, agent version, protocol version, and the default-empty
    // optional/ip/feature fields.
    let collector = Collector::new(false, false);
    let info = collector.system_info();
    assert!(!info.cpu_arch.is_empty());
    assert!(!info.agent_version.is_empty());
    assert_eq!(info.protocol_version, 0);
    assert!(info.ipv4.is_none());
    assert!(info.ipv6.is_none());
    assert!(info.features.is_empty());
    assert!(info.swap_total >= 0);
}
