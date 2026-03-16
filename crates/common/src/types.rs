use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

fn default_protocol_version() -> u32 {
    1
}

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
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Agent-facing wire type for network probe targets (minimal fields for probing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProbeTarget {
    pub target_id: String,
    pub name: String,
    pub target: String,
    pub probe_type: String,
}

/// Aggregated result from one probe round for one target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProbeResultData {
    pub target_id: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: u32,
    pub packet_received: u32,
    pub timestamp: DateTime<Utc>,
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
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    pub modified: i64,
    pub permissions: Option<String>,
    pub owner: Option<String>,
    pub group: Option<String>,
}
