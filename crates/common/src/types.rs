use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_name: String,
    pub cpu_cores: i32,
    pub cpu_arch: String,
    pub os: String,
    pub kernel_version: String,
    pub mem_total: i64,
    pub swap_total: i64,
    pub disk_total: i64,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub virtualization: Option<String>,
    pub agent_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemReport {
    pub cpu: f64,
    pub mem_used: i64,
    pub swap_used: i64,
    pub disk_used: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub uptime: u64,
    pub temperature: Option<f64>,
    pub gpu: Option<GpuReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuReport {
    pub count: i32,
    pub average_usage: f64,
    pub detailed_info: Vec<GpuInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub mem_total: i64,
    pub mem_used: i64,
    pub utilization: f64,
    pub temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingTaskConfig {
    pub task_id: String,
    pub probe_type: String,
    pub target: String,
    pub interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub task_id: String,
    pub latency: f64,
    pub success: bool,
    pub error: Option<String>,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub output: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub last_active: i64,
    pub uptime: u64,
    pub cpu: f64,
    pub mem_used: i64,
    pub mem_total: i64,
    pub swap_used: i64,
    pub swap_total: i64,
    pub disk_used: i64,
    pub disk_total: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub cpu_name: Option<String>,
    pub os: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
}
