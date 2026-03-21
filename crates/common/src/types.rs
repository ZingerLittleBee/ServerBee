use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

fn default_protocol_version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
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
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DiskIo {
    pub name: String,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
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
    #[serde(default)]
    pub disk_io: Option<Vec<DiskIo>>,
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
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TracerouteHop {
    pub hop: u8,
    pub ip: Option<String>,
    pub hostname: Option<String>,
    pub rtt1: Option<f64>, // ms
    pub rtt2: Option<f64>,
    pub rtt3: Option<f64>,
    pub asn: Option<String>,
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
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_system_info_features_default() {
        let json = r#"{"cpu_name":"x86","cpu_cores":4,"cpu_arch":"x86_64","os":"linux","kernel_version":"6.1","mem_total":8000,"swap_total":4000,"disk_total":50000,"agent_version":"0.4.0","protocol_version":2}"#;
        let info: SystemInfo = serde_json::from_str(json).unwrap();
        assert!(info.features.is_empty());
    }

    #[test]
    fn test_system_info_features_present() {
        let json = r#"{"cpu_name":"x86","cpu_cores":4,"cpu_arch":"x86_64","os":"linux","kernel_version":"6.1","mem_total":8000,"swap_total":4000,"disk_total":50000,"agent_version":"0.4.0","protocol_version":3,"features":["docker"]}"#;
        let info: SystemInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.features, vec!["docker"]);
    }

    #[test]
    fn test_network_interface_serialization() {
        let iface = NetworkInterface {
            name: "eth0".to_string(),
            ipv4: vec!["192.168.1.100".to_string(), "10.0.0.1".to_string()],
            ipv6: vec!["fe80::1".to_string()],
        };
        let json = serde_json::to_string(&iface).unwrap();
        let parsed: NetworkInterface = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "eth0");
        assert_eq!(parsed.ipv4.len(), 2);
        assert_eq!(parsed.ipv4[0], "192.168.1.100");
        assert_eq!(parsed.ipv4[1], "10.0.0.1");
        assert_eq!(parsed.ipv6.len(), 1);
        assert_eq!(parsed.ipv6[0], "fe80::1");
        assert_eq!(parsed, iface, "NetworkInterface should implement PartialEq correctly");
    }

    #[test]
    fn test_system_report_without_disk_io_defaults_to_none() {
        let legacy = json!({
            "cpu": 1.0,
            "mem_used": 0,
            "swap_used": 0,
            "disk_used": 0,
            "net_in_speed": 0,
            "net_out_speed": 0,
            "net_in_transfer": 0,
            "net_out_transfer": 0,
            "load1": 0.0,
            "load5": 0.0,
            "load15": 0.0,
            "tcp_conn": 0,
            "udp_conn": 0,
            "process_count": 0,
            "uptime": 0,
            "temperature": null,
            "gpu": null
        });

        let report: SystemReport = serde_json::from_value(legacy).unwrap();

        assert!(report.disk_io.is_none());
    }
}
