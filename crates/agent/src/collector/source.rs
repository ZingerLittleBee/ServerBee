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
            os: resolve_os(),
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

/// Resolve the OS string for `SystemInfo`.
///
/// In Docker agent mode the host's `/etc/os-release` is bind-mounted at
/// `/host/etc/os-release:ro`; prefer it over sysinfo, which reads the
/// container's own `/etc/os-release` (e.g. Alpine) and reports the wrong OS.
fn resolve_os() -> String {
    resolve_os_with_host(read_host_os_release())
}

/// Same as [`resolve_os`] but with the host `/etc/os-release` content injected,
/// so tests can assert precedence without touching the filesystem.
fn resolve_os_with_host(host: Option<String>) -> String {
    host.unwrap_or_else(|| System::long_os_version().unwrap_or_default())
}

fn read_host_os_release() -> Option<String> {
    parse_os_release(&std::fs::read_to_string("/host/etc/os-release").ok()?)
}

fn parse_os_release(content: &str) -> Option<String> {
    let mut name = None;
    let mut version = None;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return Some(format!("Linux ({})", unquote_os_release_value(value)));
        }
        if name.is_none()
            && let Some(value) = line.strip_prefix("NAME=")
        {
            name = Some(unquote_os_release_value(value).to_string());
        }
        if version.is_none()
            && let Some(value) = line.strip_prefix("VERSION=")
        {
            version = Some(unquote_os_release_value(value).to_string());
        }
    }
    match (name, version) {
        (Some(n), Some(v)) => Some(format!("Linux ({n} {v})")),
        (Some(n), None) => Some(format!("Linux ({n})")),
        _ => None,
    }
}

fn unquote_os_release_value(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_os_release_prefers_pretty_name() {
        let content = "NAME=\"Ubuntu\"\nVERSION=\"22.04 LTS\"\nPRETTY_NAME=\"Ubuntu 22.04.4 LTS\"";
        assert_eq!(
            parse_os_release(content),
            Some("Linux (Ubuntu 22.04.4 LTS)".to_string())
        );
    }

    #[test]
    fn parse_os_release_falls_back_to_name_and_version() {
        let content = "NAME=\"Debian GNU/Linux\"\nVERSION=\"12 (bookworm)\"";
        assert_eq!(
            parse_os_release(content),
            Some("Linux (Debian GNU/Linux 12 (bookworm))".to_string())
        );
    }

    #[test]
    fn parse_os_release_name_only() {
        let content = "NAME=Alpine Linux";
        assert_eq!(
            parse_os_release(content),
            Some("Linux (Alpine Linux)".to_string())
        );
    }

    #[test]
    fn parse_os_release_empty_returns_none() {
        assert_eq!(parse_os_release(""), None);
    }

    #[test]
    fn parse_os_release_no_relevant_keys_returns_none() {
        assert_eq!(parse_os_release("HOME=/root\nSHELL=/bin/sh"), None);
    }

    #[test]
    fn resolve_os_with_host_prefers_host_over_sysinfo() {
        let host = Some("Linux (Ubuntu 22.04.4 LTS)".to_string());
        let os = resolve_os_with_host(host);
        assert_eq!(os, "Linux (Ubuntu 22.04.4 LTS)");
    }

    #[test]
    fn resolve_os_with_host_falls_back_to_sysinfo_when_host_absent() {
        let os = resolve_os_with_host(None);
        assert!(!os.is_empty());
    }
}
