mod cpu;
mod disk;
mod gpu;
mod load;
mod memory;
mod network;
mod process;
mod temperature;

use std::time::Instant;

use serverbee_common::types::{SystemInfo, SystemReport};
use sysinfo::{Networks, System};

pub struct Collector {
    sys: System,
    networks: Networks,
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
            prev_net_in: net_in,
            prev_net_out: net_out,
            prev_time: Instant::now(),
            enable_temperature,
            enable_gpu,
        }
    }

    pub fn collect(&mut self) -> SystemReport {
        self.sys.refresh_all();
        self.networks.refresh(true);

        let elapsed = self.prev_time.elapsed().as_secs_f64().max(1.0);
        let (net_in, net_out) = network::total_bytes(&self.networks);
        let net_in_speed = (net_in.saturating_sub(self.prev_net_in) as f64 / elapsed) as i64;
        let net_out_speed = (net_out.saturating_sub(self.prev_net_out) as f64 / elapsed) as i64;

        self.prev_net_in = net_in;
        self.prev_net_out = net_out;
        self.prev_time = Instant::now();

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
            virtualization: None,
            agent_version: serverbee_common::constants::VERSION.to_string(),
        }
    }
}
