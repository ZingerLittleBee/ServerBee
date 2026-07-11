//! The host-reading seam behind report assembly.
//!
//! [`MetricsSource`] is everything `Collector` needs to read from the host;
//! [`SysinfoSource`] is the production adapter wrapping the sysinfo handles
//! and the per-metric submodules. Tests drive the assembly logic (rate
//! differencing, counter-reset saturation, the elapsed guard, enable gating)
//! through a fake source instead of the real machine.

use std::collections::HashMap;

use serverbee_common::types::{DiskIo, GpuReport, SystemInfo};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System};

use super::{cpu, disk, disk_io, load, memory, network, process, temperature, virtualization};

/// One connection's window onto the host: raw gauges, cumulative counters,
/// and windowed per-device I/O rates. Implementations own whatever handles
/// and previous-sample state the readings require.
pub(crate) trait MetricsSource {
    /// Refresh whatever underlying handles need refreshing before a sample.
    fn refresh(&mut self);
    fn cpu_usage(&self) -> f64;
    fn mem_used(&self) -> i64;
    fn swap_used(&self) -> i64;
    fn disk_used(&self) -> i64;
    /// Cumulative interface byte counters since boot (in, out).
    fn net_total_bytes(&self) -> (u64, u64);
    /// (load1, load5, load15)
    fn load_averages(&self) -> (f64, f64, f64);
    fn tcp_connections(&self) -> i32;
    fn udp_connections(&self) -> i32;
    fn process_count(&self) -> i32;
    fn uptime(&self) -> u64;
    /// Per-device I/O rates over the elapsed window, when the platform
    /// exposes them.
    fn disk_io(&mut self, elapsed: f64) -> Option<Vec<DiskIo>>;
    fn temperature(&self) -> Option<f64>;
    fn gpu(&self) -> Option<GpuReport>;
}

/// Production adapter: sysinfo handles plus the per-metric submodules.
pub(crate) struct SysinfoSource {
    sys: System,
    networks: Networks,
    prev_disk_io: HashMap<String, disk_io::DiskCounters>,
}

impl SysinfoSource {
    pub(crate) fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();
        Self {
            sys,
            networks,
            prev_disk_io: HashMap::new(),
        }
    }

    /// Static host facts for `SystemInfo`; not part of the [`MetricsSource`]
    /// seam because nothing in the assembly transforms them.
    pub(crate) fn system_info(&self) -> SystemInfo {
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

impl MetricsSource for SysinfoSource {
    fn refresh(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        self.networks.refresh(true);
    }

    fn cpu_usage(&self) -> f64 {
        cpu::usage(&self.sys)
    }

    fn mem_used(&self) -> i64 {
        memory::mem_used(&self.sys)
    }

    fn swap_used(&self) -> i64 {
        memory::swap_used(&self.sys)
    }

    fn disk_used(&self) -> i64 {
        disk::used()
    }

    fn net_total_bytes(&self) -> (u64, u64) {
        network::total_bytes(&self.networks)
    }

    fn load_averages(&self) -> (f64, f64, f64) {
        (load::load1(), load::load5(), load::load15())
    }

    fn tcp_connections(&self) -> i32 {
        process::tcp_connections()
    }

    fn udp_connections(&self) -> i32 {
        process::udp_connections()
    }

    fn process_count(&self) -> i32 {
        process::count(&self.sys)
    }

    fn uptime(&self) -> u64 {
        System::uptime()
    }

    fn disk_io(&mut self, elapsed: f64) -> Option<Vec<DiskIo>> {
        disk_io::collect(elapsed, &mut self.prev_disk_io)
    }

    fn temperature(&self) -> Option<f64> {
        temperature::get_temperature()
    }

    fn gpu(&self) -> Option<GpuReport> {
        super::gpu::get_gpu_report()
    }
}
