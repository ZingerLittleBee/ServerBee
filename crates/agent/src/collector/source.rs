//! The host-reading seam behind report assembly.
//!
//! [`MetricsSource`] is everything `Collector` needs to read from the host;
//! [`SysinfoSource`] is the production adapter wrapping the sysinfo handles
//! and the per-metric submodules. Tests drive the assembly logic (rate
//! differencing, counter-reset saturation, the elapsed guard, enable gating)
//! through a fake source instead of the real machine.

use std::time::{Duration, Instant};

use serverbee_common::types::{DiskIo, GpuReport, SystemInfo};
use sysinfo::{
    Components, CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind,
    System,
};

use super::{disk, disk_io, gpu, network, process, temperature, virtualization};

const RESOURCE_LIST_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

pub(crate) trait MetricsSource {
    fn refresh(&mut self);
    fn cpu_usage(&self) -> f64;
    fn mem_used(&self) -> i64;
    fn swap_used(&self) -> i64;
    fn disk_used(&self) -> i64;
    fn net_total_bytes(&self) -> (u64, u64);
    fn load_averages(&self) -> (f64, f64, f64);
    fn tcp_connections(&mut self) -> i32;
    fn udp_connections(&mut self) -> i32;
    fn process_count(&self) -> i32;
    fn uptime(&self) -> u64;
    fn disk_io(&mut self, elapsed: f64) -> Option<Vec<DiskIo>>;
    fn temperature(&self) -> Option<f64>;
    fn gpu(&mut self) -> Option<GpuReport>;
}

pub(crate) struct SysinfoSource {
    sys: System,
    networks: Networks,
    disks: Disks,
    components: Option<Components>,
    disk_io_state: disk_io::DiskIoState,
    gpu_collector: gpu::GpuCollector,
    proc_net_buf: Vec<u8>,
    last_list_refresh: Instant,
    exclude_tailscale_traffic: bool,
}

impl SysinfoSource {
    pub(crate) fn new(enable_temperature: bool, exclude_tailscale_traffic: bool) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        let networks = Networks::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();
        let components = enable_temperature.then(Components::new_with_refreshed_list);
        Self {
            sys,
            networks,
            disks,
            components,
            disk_io_state: disk_io::DiskIoState::new(),
            gpu_collector: gpu::GpuCollector::new(),
            proc_net_buf: vec![0u8; 8192],
            last_list_refresh: Instant::now(),
            exclude_tailscale_traffic,
        }
    }

    pub(crate) fn system_info(&self) -> SystemInfo {
        SystemInfo {
            protocol_version: 0,
            cpu_name: self
                .sys
                .cpus()
                .first()
                .map(|cpu| cpu.brand().to_string())
                .unwrap_or_default(),
            cpu_cores: self.sys.cpus().len() as i32,
            cpu_arch: std::env::consts::ARCH.to_string(),
            os: resolve_os(),
            kernel_version: System::kernel_version().unwrap_or_default(),
            mem_total: self.sys.total_memory() as i64,
            swap_total: self.sys.total_swap() as i64,
            disk_total: disk::total_from_disks(&self.disks),
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
        self.networks.refresh(true);

        let refresh_lists = self.last_list_refresh.elapsed() >= RESOURCE_LIST_REFRESH_INTERVAL;
        if refresh_lists {
            self.disks.refresh(true);
            if let Some(components) = self.components.as_mut() {
                components.refresh(true);
            }
            self.last_list_refresh = Instant::now();
        } else {
            let disk_kind = DiskRefreshKind::nothing().with_storage();
            for disk in self.disks.list_mut() {
                disk.refresh_specifics(disk_kind);
            }
            if let Some(components) = self.components.as_mut() {
                for component in components.list_mut() {
                    component.refresh();
                }
            }
        }
    }

    fn cpu_usage(&self) -> f64 {
        self.sys.global_cpu_usage() as f64
    }

    fn mem_used(&self) -> i64 {
        self.sys.used_memory() as i64
    }

    fn swap_used(&self) -> i64 {
        self.sys.used_swap() as i64
    }

    fn disk_used(&self) -> i64 {
        disk::used_from_disks(&self.disks)
    }

    fn net_total_bytes(&self) -> (u64, u64) {
        network::total_bytes(&self.networks, self.exclude_tailscale_traffic)
    }

    fn load_averages(&self) -> (f64, f64, f64) {
        let load = System::load_average();
        (load.one, load.five, load.fifteen)
    }

    fn tcp_connections(&mut self) -> i32 {
        process::tcp_connections_with_buf(&mut self.proc_net_buf)
    }

    fn udp_connections(&mut self) -> i32 {
        process::udp_connections_with_buf(&mut self.proc_net_buf)
    }

    fn process_count(&self) -> i32 {
        process::count()
    }

    fn uptime(&self) -> u64 {
        System::uptime()
    }

    fn disk_io(&mut self, elapsed: f64) -> Option<Vec<DiskIo>> {
        self.disk_io_state.collect(elapsed)
    }

    fn temperature(&self) -> Option<f64> {
        self.components
            .as_ref()
            .and_then(temperature::get_temperature_from_components)
    }

    fn gpu(&mut self) -> Option<GpuReport> {
        self.gpu_collector.collect()
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
