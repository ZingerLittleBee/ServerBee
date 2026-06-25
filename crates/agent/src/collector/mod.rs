mod cpu;
mod disk;
mod disk_io;
mod gpu;
mod load;
mod memory;
mod network;
mod process;
mod temperature;
pub mod virtualization;

use std::time::Instant;

use serverbee_common::types::{SystemInfo, SystemReport};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System};

pub struct Collector {
    sys: System,
    networks: Networks,
    prev_disk_io: std::collections::HashMap<String, disk_io::DiskCounters>,
    prev_net_in: u64,
    prev_net_out: u64,
    prev_time: Instant,
    enable_temperature: bool,
    enable_gpu: bool,
}

impl Collector {
    pub fn new(enable_temperature: bool, enable_gpu: bool) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();
        let (net_in, net_out) = network::total_bytes(&networks);
        Self {
            sys,
            networks,
            prev_disk_io: std::collections::HashMap::new(),
            prev_net_in: net_in,
            prev_net_out: net_out,
            prev_time: Instant::now(),
            enable_temperature,
            enable_gpu,
        }
    }

    pub fn collect(&mut self) -> SystemReport {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        self.networks.refresh(true);

        let elapsed = self.prev_time.elapsed().as_secs_f64().max(1.0);
        let (net_in, net_out) = network::total_bytes(&self.networks);
        let net_in_speed = (net_in.saturating_sub(self.prev_net_in) as f64 / elapsed) as i64;
        let net_out_speed = (net_out.saturating_sub(self.prev_net_out) as f64 / elapsed) as i64;

        self.prev_net_in = net_in;
        self.prev_net_out = net_out;
        self.prev_time = Instant::now();

        let disk_io = disk_io::collect(elapsed, &mut self.prev_disk_io);

        let temperature = if self.enable_temperature {
            temperature::get_temperature()
        } else {
            None
        };

        SystemReport {
            cpu: cpu::usage(&self.sys),
            mem_used: memory::mem_used(&self.sys),
            swap_used: memory::swap_used(&self.sys),
            disk_used: disk::used(),
            net_in_speed,
            net_out_speed,
            net_in_transfer: net_in as i64,
            net_out_transfer: net_out as i64,
            load1: load::load1(),
            load5: load::load5(),
            load15: load::load15(),
            tcp_conn: process::tcp_connections(),
            udp_conn: process::udp_connections(),
            process_count: process::count(&self.sys),
            uptime: System::uptime(),
            disk_io,
            temperature,
            gpu: if self.enable_gpu {
                gpu::get_gpu_report()
            } else {
                None
            },
        }
    }

    pub fn system_info(&self) -> SystemInfo {
        SystemInfo {
            protocol_version: 0,
            cpu_name: cpu::name(&self.sys),
            cpu_cores: cpu::cores(&self.sys),
            cpu_arch: cpu::arch(),
            os: System::long_os_version().unwrap_or_default(),
            kernel_version: System::kernel_version().unwrap_or_default(),
            mem_total: memory::mem_total(&self.sys),
            swap_total: memory::swap_total(&self.sys),
            disk_total: disk::total(),
            ipv4: None,
            ipv6: None,
            virtualization: virtualization::detect(),
            agent_version: serverbee_common::constants::VERSION.to_string(),
            features: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod branch_tests {
    use super::*;

    #[test]
    fn collect_with_temperature_disabled_yields_none() {
        // When `enable_temperature` is false the collector skips the sensor read
        // entirely and reports `temperature: None` (the disabled branch in
        // `Collector::collect`).
        let mut collector = Collector::new(false, false);
        let report = collector.collect();
        assert!(report.temperature.is_none());
    }

    #[test]
    fn collect_with_gpu_enabled_does_not_panic() {
        // With `enable_gpu` true the collector invokes `gpu::get_gpu_report()`.
        // Without the `gpu` cargo feature (the default, and the case on CI hosts
        // with no NVIDIA GPU) this returns None, but the branch must be reached
        // and must not panic. When a report is present, validate its invariants.
        let mut collector = Collector::new(false, true);
        let report = collector.collect();
        if let Some(gpu) = report.gpu {
            assert_eq!(gpu.count as usize, gpu.detailed_info.len());
            assert!(gpu.average_usage.is_finite());
        }
    }

    #[test]
    fn collect_report_field_invariants() {
        // Cross-field invariants on a freshly collected report: connection and
        // network counters are non-negative and load averages are finite.
        let mut collector = Collector::new(false, false);
        let report = collector.collect();
        assert!(report.tcp_conn >= 0);
        assert!(report.udp_conn >= 0);
        assert!(report.net_in_transfer >= 0);
        assert!(report.net_out_transfer >= 0);
        assert!(report.net_in_speed >= 0);
        assert!(report.net_out_speed >= 0);
        assert!(report.load1 >= 0.0 && report.load1.is_finite());
        assert!(report.load5 >= 0.0 && report.load5.is_finite());
        assert!(report.load15 >= 0.0 && report.load15.is_finite());
        assert!(report.swap_used >= 0);
    }

    #[test]
    fn system_info_static_assembly_fields() {
        // Assert the `system_info` assembly fields not covered by the existing
        // populated-fields test (arch, agent version, protocol version, and the
        // default-empty optional/ip/feature fields).
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
}
