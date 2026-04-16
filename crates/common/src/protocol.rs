use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::constants::CapabilityDeniedReason;
use crate::docker_types::*;
use crate::types::{
    FileEntry, NetworkInterface, NetworkProbeResultData, NetworkProbeTarget, PingResult,
    PingTaskConfig, SystemInfo, SystemReport, TaskResult, TracerouteHop,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum UpgradeStage {
    Downloading,
    Verifying,
    PreFlight,
    Installing,
    Restarting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum UpgradeStatus {
    Running,
    Succeeded,
    Failed,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UpgradeJobDto {
    pub server_id: String,
    pub job_id: String,
    pub target_version: String,
    pub stage: UpgradeStage,
    pub status: UpgradeStatus,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub backup_path: Option<String>,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
}

/// Agent -> Server messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    SystemInfo {
        msg_id: String,
        #[serde(flatten)]
        info: SystemInfo,
        #[serde(default)]
        agent_local_capabilities: Option<u32>,
    },
    Report(SystemReport),
    PingResult(PingResult),
    TaskResult {
        msg_id: String,
        #[serde(flatten)]
        result: TaskResult,
    },
    TerminalOutput {
        session_id: String,
        data: String, // base64 encoded
    },
    TerminalStarted {
        session_id: String,
    },
    TerminalError {
        session_id: String,
        error: String,
    },
    CapabilityDenied {
        msg_id: Option<String>,
        session_id: Option<String>,
        capability: String,
        reason: CapabilityDeniedReason,
    },
    RebindIdentityAck {
        job_id: String,
    },
    RebindIdentityFailed {
        job_id: String,
        error: String,
    },
    NetworkProbeResults {
        results: Vec<NetworkProbeResultData>,
    },
    // File management responses
    FileListResult {
        msg_id: String,
        path: String,
        entries: Vec<FileEntry>,
        error: Option<String>,
    },
    FileStatResult {
        msg_id: String,
        entry: Option<FileEntry>,
        error: Option<String>,
    },
    FileReadResult {
        msg_id: String,
        content: Option<String>,
        error: Option<String>,
    },
    FileOpResult {
        msg_id: String,
        success: bool,
        error: Option<String>,
    },
    FileDownloadReady {
        transfer_id: String,
        size: u64,
    },
    FileDownloadChunk {
        transfer_id: String,
        offset: u64,
        data: String,
    },
    FileDownloadEnd {
        transfer_id: String,
    },
    FileDownloadError {
        transfer_id: String,
        error: String,
    },
    FileUploadAck {
        transfer_id: String,
        offset: u64,
    },
    FileUploadComplete {
        transfer_id: String,
    },
    FileUploadError {
        transfer_id: String,
        error: String,
    },
    // Docker responses
    DockerInfo {
        msg_id: Option<String>,
        info: DockerSystemInfo,
    },
    DockerContainers {
        msg_id: Option<String>,
        containers: Vec<DockerContainer>,
    },
    DockerStats {
        stats: Vec<DockerContainerStats>,
    },
    DockerLog {
        session_id: String,
        entries: Vec<DockerLogEntry>,
    },
    DockerEvent {
        event: DockerEventInfo,
    },
    FeaturesUpdate {
        features: Vec<String>,
    },
    DockerUnavailable {
        #[serde(skip_serializing_if = "Option::is_none")]
        msg_id: Option<String>,
    },
    DockerNetworks {
        msg_id: String,
        networks: Vec<DockerNetwork>,
    },
    DockerVolumes {
        msg_id: String,
        volumes: Vec<DockerVolume>,
    },
    DockerActionResult {
        msg_id: String,
        success: bool,
        error: Option<String>,
    },
    IpChanged {
        ipv4: Option<String>,
        ipv6: Option<String>,
        interfaces: Vec<NetworkInterface>,
    },
    TracerouteResult {
        request_id: String,
        target: String,
        hops: Vec<TracerouteHop>,
        completed: bool,
        error: Option<String>,
    },
    UpgradeProgress {
        msg_id: String,
        #[serde(default)]
        job_id: Option<String>,
        target_version: String,
        stage: UpgradeStage,
    },
    UpgradeResult {
        msg_id: String,
        #[serde(default)]
        job_id: Option<String>,
        target_version: String,
        stage: UpgradeStage,
        error: String,
        #[serde(default)]
        backup_path: Option<String>,
    },
    Pong,
}

/// Server -> Agent messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        server_id: String,
        protocol_version: u32,
        report_interval: u32,
        #[serde(default)]
        capabilities: Option<u32>,
    },
    Ack {
        msg_id: String,
    },
    PingTasksSync {
        tasks: Vec<PingTaskConfig>,
    },
    Exec {
        task_id: String,
        command: String,
        timeout: Option<u32>,
    },
    TerminalOpen {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    TerminalInput {
        session_id: String,
        data: String, // base64 encoded
    },
    TerminalResize {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    TerminalClose {
        session_id: String,
    },
    NetworkProbeSync {
        targets: Vec<NetworkProbeTarget>,
        interval: u32,
        packet_count: u32,
    },
    // File management commands
    FileList {
        msg_id: String,
        path: String,
    },
    FileDelete {
        msg_id: String,
        path: String,
        recursive: bool,
    },
    FileMkdir {
        msg_id: String,
        path: String,
    },
    FileMove {
        msg_id: String,
        from: String,
        to: String,
    },
    FileStat {
        msg_id: String,
        path: String,
    },
    FileRead {
        msg_id: String,
        path: String,
        max_size: u64,
    },
    FileWrite {
        msg_id: String,
        path: String,
        content: String,
    },
    FileDownloadStart {
        transfer_id: String,
        path: String,
    },
    FileDownloadCancel {
        transfer_id: String,
    },
    FileUploadStart {
        transfer_id: String,
        path: String,
        size: u64,
    },
    FileUploadChunk {
        transfer_id: String,
        offset: u64,
        data: String,
    },
    FileUploadEnd {
        transfer_id: String,
    },
    // Docker commands
    DockerListContainers {
        msg_id: String,
    },
    DockerStartStats {
        interval_secs: u32,
    },
    DockerStopStats,
    DockerContainerAction {
        msg_id: String,
        container_id: String,
        action: DockerAction,
    },
    DockerLogsStart {
        session_id: String,
        container_id: String,
        tail: Option<u64>,
        follow: bool,
    },
    DockerLogsStop {
        session_id: String,
    },
    DockerEventsStart,
    DockerEventsStop,
    DockerGetInfo {
        msg_id: String,
    },
    DockerListNetworks {
        msg_id: String,
    },
    DockerListVolumes {
        msg_id: String,
    },
    Traceroute {
        request_id: String,
        target: String,
        max_hops: u8,
    },
    Ping,
    Upgrade {
        version: String,
        download_url: String,
        sha256: String,
        #[serde(default)]
        job_id: Option<String>,
    },
    RebindIdentity {
        job_id: String,
        target_server_id: String,
        token: String,
    },
    CapabilitiesSync {
        capabilities: u32,
    },
}

/// Server -> Browser messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserMessage {
    FullSync {
        servers: Vec<crate::types::ServerStatus>,
        #[serde(default)]
        upgrades: Vec<UpgradeJobDto>,
    },
    Update {
        servers: Vec<crate::types::ServerStatus>,
    },
    ServerOnline {
        server_id: String,
    },
    ServerOffline {
        server_id: String,
    },
    CapabilitiesChanged {
        server_id: String,
        capabilities: u32,
        agent_local_capabilities: Option<u32>,
        effective_capabilities: Option<u32>,
    },
    AgentInfoUpdated {
        server_id: String,
        protocol_version: u32,
        #[serde(default)]
        agent_version: Option<String>,
    },
    UpgradeProgress {
        server_id: String,
        job_id: String,
        target_version: String,
        stage: UpgradeStage,
    },
    UpgradeResult {
        server_id: String,
        job_id: String,
        target_version: String,
        status: UpgradeStatus,
        stage: Option<UpgradeStage>,
        error: Option<String>,
        backup_path: Option<String>,
    },
    NetworkProbeUpdate {
        server_id: String,
        results: Vec<NetworkProbeResultData>,
    },
    // Docker broadcasts
    DockerUpdate {
        server_id: String,
        containers: Vec<DockerContainer>,
        stats: Option<Vec<DockerContainerStats>>,
    },
    DockerEvent {
        server_id: String,
        event: DockerEventInfo,
    },
    DockerAvailabilityChanged {
        server_id: String,
        available: bool,
    },
    ServerIpChanged {
        server_id: String,
        old_ipv4: Option<String>,
        new_ipv4: Option<String>,
        old_ipv6: Option<String>,
        new_ipv6: Option<String>,
        old_remote_addr: Option<String>,
        new_remote_addr: Option<String>,
    },
}

/// Browser -> Server messages (upstream via browser WS)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserClientMessage {
    DockerSubscribe { server_id: String },
    DockerUnsubscribe { server_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiskIo;

    #[test]
    fn test_welcome_without_capabilities_deserializes() {
        let json =
            r#"{"type":"welcome","server_id":"s1","protocol_version":1,"report_interval":3}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Welcome { capabilities, .. } => {
                assert_eq!(capabilities, None);
            }
            _ => panic!("Expected Welcome"),
        }
    }

    #[test]
    fn test_welcome_with_capabilities_deserializes() {
        let json = r#"{"type":"welcome","server_id":"s1","protocol_version":2,"report_interval":3,"capabilities":56}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Welcome { capabilities, .. } => {
                assert_eq!(capabilities, Some(56));
            }
            _ => panic!("Expected Welcome"),
        }
    }

    #[test]
    fn test_capabilities_sync_round_trip() {
        let msg = ServerMessage::CapabilitiesSync { capabilities: 7 };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::CapabilitiesSync { capabilities } => {
                assert_eq!(capabilities, 7);
            }
            _ => panic!("Expected CapabilitiesSync"),
        }
    }

    #[test]
    fn test_rebind_identity_round_trip() {
        let msg = ServerMessage::RebindIdentity {
            job_id: "job-1".to_string(),
            target_server_id: "server-1".to_string(),
            token: "token-123".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::RebindIdentity {
                job_id,
                target_server_id,
                token,
            } => {
                assert_eq!(job_id, "job-1");
                assert_eq!(target_server_id, "server-1");
                assert_eq!(token, "token-123");
            }
            _ => panic!("Expected RebindIdentity"),
        }
    }

    #[test]
    fn test_rebind_identity_ack_round_trip() {
        let msg = AgentMessage::RebindIdentityAck {
            job_id: "job-1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::RebindIdentityAck { job_id } => {
                assert_eq!(job_id, "job-1");
            }
            _ => panic!("Expected RebindIdentityAck"),
        }
    }

    #[test]
    fn test_rebind_identity_failed_round_trip() {
        let msg = AgentMessage::RebindIdentityFailed {
            job_id: "job-1".to_string(),
            error: "permission denied".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::RebindIdentityFailed { job_id, error } => {
                assert_eq!(job_id, "job-1");
                assert_eq!(error, "permission denied");
            }
            _ => panic!("Expected RebindIdentityFailed"),
        }
    }

    #[test]
    fn test_capability_denied_round_trip() {
        let msg = AgentMessage::CapabilityDenied {
            msg_id: Some("task-1".to_string()),
            session_id: None,
            capability: "exec".to_string(),
            reason: CapabilityDeniedReason::AgentCapabilityDisabled,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::CapabilityDenied {
                msg_id,
                session_id,
                capability,
                reason,
            } => {
                assert_eq!(msg_id, Some("task-1".to_string()));
                assert_eq!(session_id, None);
                assert_eq!(capability, "exec");
                assert_eq!(reason, CapabilityDeniedReason::AgentCapabilityDisabled);
            }
            _ => panic!("Expected CapabilityDenied"),
        }
    }

    #[test]
    fn test_system_info_without_protocol_version() {
        let json = r#"{"type":"system_info","msg_id":"m1","cpu_name":"Intel","cpu_cores":4,"cpu_arch":"x86_64","os":"Linux","kernel_version":"5.4","mem_total":8000000000,"swap_total":0,"disk_total":100000000000,"agent_version":"0.1.0"}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::SystemInfo {
                info,
                agent_local_capabilities,
                ..
            } => {
                assert_eq!(info.protocol_version, 1);
                assert_eq!(agent_local_capabilities, None);
            }
            _ => panic!("Expected SystemInfo"),
        }
    }

    #[test]
    fn test_system_info_round_trip_with_agent_local_capabilities() {
        let msg = AgentMessage::SystemInfo {
            msg_id: "m1".to_string(),
            info: SystemInfo {
                cpu_name: "Intel".to_string(),
                cpu_cores: 4,
                cpu_arch: "x86_64".to_string(),
                os: "Linux".to_string(),
                kernel_version: "6.8".to_string(),
                mem_total: 8_000_000_000,
                swap_total: 0,
                disk_total: 100_000_000_000,
                ipv4: Some("192.0.2.10".to_string()),
                ipv6: None,
                virtualization: Some("kvm".to_string()),
                agent_version: "0.1.0".to_string(),
                protocol_version: 3,
                features: vec!["docker".to_string()],
            },
            agent_local_capabilities: Some(64),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::SystemInfo {
                agent_local_capabilities,
                ..
            } => {
                assert_eq!(agent_local_capabilities, Some(64));
            }
            _ => panic!("Expected SystemInfo"),
        }
    }

    #[test]
    fn test_browser_capabilities_changed_round_trip_with_effective_caps() {
        let msg = BrowserMessage::CapabilitiesChanged {
            server_id: "server-1".to_string(),
            capabilities: 7,
            agent_local_capabilities: Some(64),
            effective_capabilities: Some(0),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            BrowserMessage::CapabilitiesChanged {
                server_id,
                capabilities,
                agent_local_capabilities,
                effective_capabilities,
            } => {
                assert_eq!(server_id, "server-1");
                assert_eq!(capabilities, 7);
                assert_eq!(agent_local_capabilities, Some(64));
                assert_eq!(effective_capabilities, Some(0));
            }
            _ => panic!("Expected CapabilitiesChanged"),
        }
    }

    #[test]
    fn test_network_probe_sync_serializes() {
        let msg = ServerMessage::NetworkProbeSync {
            targets: vec![],
            interval: 60,
            packet_count: 10,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::NetworkProbeSync {
                interval,
                packet_count,
                ..
            } => {
                assert_eq!(interval, 60);
                assert_eq!(packet_count, 10);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_network_probe_results_serializes() {
        let msg = AgentMessage::NetworkProbeResults { results: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::NetworkProbeResults { results } => assert!(results.is_empty()),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_report_with_disk_io_round_trip() {
        let msg = AgentMessage::Report(SystemReport {
            disk_io: Some(vec![DiskIo {
                name: "sda".to_string(),
                read_bytes_per_sec: 1024,
                write_bytes_per_sec: 2048,
            }]),
            ..Default::default()
        });

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            AgentMessage::Report(report) => {
                assert_eq!(report.disk_io.unwrap()[0].name, "sda");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_list_round_trip() {
        let msg = ServerMessage::FileList {
            msg_id: "m1".into(),
            path: "/home".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("file_list"));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::FileList { msg_id, path } => {
                assert_eq!(msg_id, "m1");
                assert_eq!(path, "/home");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_list_result_round_trip() {
        use crate::types::{FileEntry, FileType};
        let entry = FileEntry {
            name: "test.txt".into(),
            path: "/home/test.txt".into(),
            file_type: FileType::File,
            size: 1024,
            modified: 1710000000,
            permissions: Some("rw-r--r--".into()),
            owner: Some("root".into()),
            group: Some("root".into()),
        };
        let msg = AgentMessage::FileListResult {
            msg_id: "m1".into(),
            path: "/home".into(),
            entries: vec![entry],
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileListResult { entries, .. } => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].name, "test.txt");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_download_chunk_round_trip() {
        let msg = AgentMessage::FileDownloadChunk {
            transfer_id: "t1".into(),
            offset: 0,
            data: "aGVsbG8=".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileDownloadChunk {
                transfer_id,
                offset,
                data,
            } => {
                assert_eq!(transfer_id, "t1");
                assert_eq!(offset, 0);
                assert_eq!(data, "aGVsbG8=");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_stat_result_round_trip() {
        use crate::types::{FileEntry, FileType};
        let msg = AgentMessage::FileStatResult {
            msg_id: "m1".into(),
            entry: Some(FileEntry {
                name: "test.txt".into(),
                path: "/home/test.txt".into(),
                file_type: FileType::File,
                size: 1024,
                modified: 1710000000,
                permissions: None,
                owner: None,
                group: None,
            }),
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileStatResult { entry, error, .. } => {
                assert!(entry.is_some());
                assert!(error.is_none());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_read_write_round_trip() {
        // Test FileRead command
        let cmd = ServerMessage::FileRead {
            msg_id: "m1".into(),
            path: "/etc/config.yaml".into(),
            max_size: 384 * 1024,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("file_read"));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::FileRead { path, max_size, .. } => {
                assert_eq!(path, "/etc/config.yaml");
                assert_eq!(max_size, 384 * 1024);
            }
            _ => panic!("Wrong variant"),
        }

        // Test FileWrite command
        let cmd = ServerMessage::FileWrite {
            msg_id: "m2".into(),
            path: "/home/config.yaml".into(),
            content: "aGVsbG8=".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("file_write"));
        let _: ServerMessage = serde_json::from_str(&json).unwrap();

        // Test FileReadResult
        let result = AgentMessage::FileReadResult {
            msg_id: "m1".into(),
            content: Some("base64data".into()),
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileReadResult { content, error, .. } => {
                assert_eq!(content, Some("base64data".into()));
                assert!(error.is_none());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_op_result_round_trip() {
        let msg = AgentMessage::FileOpResult {
            msg_id: "m1".into(),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileOpResult { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Wrong variant"),
        }

        // Test failure case
        let msg = AgentMessage::FileOpResult {
            msg_id: "m2".into(),
            success: false,
            error: Some("Permission denied".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileOpResult { success, error, .. } => {
                assert!(!success);
                assert_eq!(error.unwrap(), "Permission denied");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_download_ready_round_trip() {
        let msg = AgentMessage::FileDownloadReady {
            transfer_id: "t1".into(),
            size: 1073741824, // 1GB
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileDownloadReady { transfer_id, size } => {
                assert_eq!(transfer_id, "t1");
                assert_eq!(size, 1073741824);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_upload_messages_round_trip() {
        // FileUploadStart
        let cmd = ServerMessage::FileUploadStart {
            transfer_id: "t1".into(),
            path: "/home/backup.tar.gz".into(),
            size: 500_000_000,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("file_upload_start"));
        let _: ServerMessage = serde_json::from_str(&json).unwrap();

        // FileUploadChunk
        let cmd = ServerMessage::FileUploadChunk {
            transfer_id: "t1".into(),
            offset: 384 * 1024,
            data: "base64chunk".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let _: ServerMessage = serde_json::from_str(&json).unwrap();

        // FileUploadAck
        let ack = AgentMessage::FileUploadAck {
            transfer_id: "t1".into(),
            offset: 384 * 1024,
        };
        let json = serde_json::to_string(&ack).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileUploadAck { offset, .. } => assert_eq!(offset, 384 * 1024),
            _ => panic!("Wrong variant"),
        }

        // FileUploadComplete
        let complete = AgentMessage::FileUploadComplete {
            transfer_id: "t1".into(),
        };
        let json = serde_json::to_string(&complete).unwrap();
        assert!(json.contains("file_upload_complete"));
        let _: AgentMessage = serde_json::from_str(&json).unwrap();

        // FileUploadError
        let err = AgentMessage::FileUploadError {
            transfer_id: "t1".into(),
            error: "Disk full".into(),
        };
        let json = serde_json::to_string(&err).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::FileUploadError { error, .. } => assert_eq!(error, "Disk full"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_file_delete_mkdir_move_round_trip() {
        let cmd = ServerMessage::FileDelete {
            msg_id: "m1".into(),
            path: "/home/old".into(),
            recursive: true,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("file_delete"));
        assert!(json.contains("\"recursive\":true"));
        let _: ServerMessage = serde_json::from_str(&json).unwrap();

        let cmd = ServerMessage::FileMkdir {
            msg_id: "m2".into(),
            path: "/home/new_dir".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("file_mkdir"));
        let _: ServerMessage = serde_json::from_str(&json).unwrap();

        let cmd = ServerMessage::FileMove {
            msg_id: "m3".into(),
            from: "/home/a.txt".into(),
            to: "/home/b.txt".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("file_move"));
        let _: ServerMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_docker_agent_message_serde() {
        let msg = AgentMessage::DockerInfo {
            msg_id: None,
            info: DockerSystemInfo {
                docker_version: "27.1.1".into(),
                api_version: "1.46".into(),
                os: "linux".into(),
                arch: "x86_64".into(),
                containers_running: 5,
                containers_paused: 0,
                containers_stopped: 2,
                images: 10,
                memory_total: 8_000_000_000,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"docker_info\""));
        let _: AgentMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_docker_server_message_serde() {
        let msg = ServerMessage::DockerStartStats { interval_secs: 3 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"docker_start_stats\""));
    }

    #[test]
    fn test_browser_client_message_serde() {
        let json = r#"{"type":"docker_subscribe","server_id":"abc123"}"#;
        let msg: BrowserClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            BrowserClientMessage::DockerSubscribe { server_id } => assert_eq!(server_id, "abc123"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_features_update_serde() {
        let msg = AgentMessage::FeaturesUpdate {
            features: vec!["docker".into()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"features_update\""));
        let _: AgentMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_docker_unavailable_serde() {
        let msg = AgentMessage::DockerUnavailable {
            msg_id: Some("abc123".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"docker_unavailable\""));
        assert!(json.contains("\"msg_id\":\"abc123\""));
        let _: AgentMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_network_probe_result_with_null_latency() {
        use crate::types::NetworkProbeResultData;
        use chrono::Utc;
        let data = NetworkProbeResultData {
            target_id: "t1".into(),
            avg_latency: None,
            min_latency: None,
            max_latency: None,
            packet_loss: 1.0,
            packet_sent: 10,
            packet_received: 0,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("null"));
        let parsed: NetworkProbeResultData = serde_json::from_str(&json).unwrap();
        assert!(parsed.avg_latency.is_none());
        assert_eq!(parsed.packet_loss, 1.0);
    }

    #[test]
    fn test_ip_changed_serialization() {
        use crate::types::NetworkInterface;
        let msg = AgentMessage::IpChanged {
            ipv4: Some("1.2.3.4".to_string()),
            ipv6: None,
            interfaces: vec![NetworkInterface {
                name: "eth0".to_string(),
                ipv4: vec!["192.168.1.100".to_string()],
                ipv6: vec!["fe80::1".to_string()],
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ip_changed\""));
        assert!(json.contains("\"ipv4\":\"1.2.3.4\""));
        assert!(json.contains("\"eth0\""));

        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::IpChanged {
                ipv4,
                ipv6,
                interfaces,
            } => {
                assert_eq!(ipv4, Some("1.2.3.4".to_string()));
                assert_eq!(ipv6, None);
                assert_eq!(interfaces.len(), 1);
                assert_eq!(interfaces[0].name, "eth0");
                assert_eq!(interfaces[0].ipv4, vec!["192.168.1.100"]);
                assert_eq!(interfaces[0].ipv6, vec!["fe80::1"]);
            }
            _ => panic!("Expected IpChanged"),
        }
    }

    #[test]
    fn test_traceroute_server_message_round_trip() {
        let msg = ServerMessage::Traceroute {
            request_id: "req-1".to_string(),
            target: "8.8.8.8".to_string(),
            max_hops: 30,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"traceroute\""));
        assert!(json.contains("\"max_hops\":30"));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::Traceroute {
                request_id,
                target,
                max_hops,
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(target, "8.8.8.8");
                assert_eq!(max_hops, 30);
            }
            _ => panic!("Expected Traceroute"),
        }
    }

    #[test]
    fn test_traceroute_result_round_trip() {
        use crate::types::TracerouteHop;
        let msg = AgentMessage::TracerouteResult {
            request_id: "req-1".to_string(),
            target: "8.8.8.8".to_string(),
            hops: vec![
                TracerouteHop {
                    hop: 1,
                    ip: Some("192.168.1.1".to_string()),
                    hostname: None,
                    rtt1: Some(1.234),
                    rtt2: Some(1.456),
                    rtt3: Some(1.678),
                    asn: None,
                },
                TracerouteHop {
                    hop: 2,
                    ip: None,
                    hostname: None,
                    rtt1: None,
                    rtt2: None,
                    rtt3: None,
                    asn: None,
                },
            ],
            completed: true,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"traceroute_result\""));
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::TracerouteResult {
                request_id,
                target,
                hops,
                completed,
                error,
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(target, "8.8.8.8");
                assert_eq!(hops.len(), 2);
                assert_eq!(hops[0].hop, 1);
                assert_eq!(hops[0].ip, Some("192.168.1.1".to_string()));
                assert_eq!(hops[0].rtt1, Some(1.234));
                assert_eq!(hops[1].hop, 2);
                assert!(hops[1].ip.is_none());
                assert!(hops[1].rtt1.is_none());
                assert!(completed);
                assert!(error.is_none());
            }
            _ => panic!("Expected TracerouteResult"),
        }
    }

    #[test]
    fn test_traceroute_result_with_error_round_trip() {
        let msg = AgentMessage::TracerouteResult {
            request_id: "req-2".to_string(),
            target: "example.com".to_string(),
            hops: vec![],
            completed: true,
            error: Some("traceroute not installed".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::TracerouteResult {
                hops,
                completed,
                error,
                ..
            } => {
                assert!(hops.is_empty());
                assert!(completed);
                assert_eq!(error, Some("traceroute not installed".to_string()));
            }
            _ => panic!("Expected TracerouteResult"),
        }
    }

    #[test]
    fn test_server_ip_changed_serialization() {
        let msg = BrowserMessage::ServerIpChanged {
            server_id: "srv-1".to_string(),
            old_ipv4: Some("1.2.3.4".to_string()),
            new_ipv4: Some("5.6.7.8".to_string()),
            old_ipv6: None,
            new_ipv6: Some("2001:db8::1".to_string()),
            old_remote_addr: Some("1.2.3.4:54321".to_string()),
            new_remote_addr: Some("5.6.7.8:12345".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"server_ip_changed\""));
        assert!(json.contains("\"server_id\":\"srv-1\""));
        assert!(json.contains("\"old_ipv4\":\"1.2.3.4\""));
        assert!(json.contains("\"new_ipv4\":\"5.6.7.8\""));

        let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            BrowserMessage::ServerIpChanged {
                server_id,
                old_ipv4,
                new_ipv4,
                old_ipv6,
                new_ipv6,
                old_remote_addr,
                new_remote_addr,
            } => {
                assert_eq!(server_id, "srv-1");
                assert_eq!(old_ipv4, Some("1.2.3.4".to_string()));
                assert_eq!(new_ipv4, Some("5.6.7.8".to_string()));
                assert_eq!(old_ipv6, None);
                assert_eq!(new_ipv6, Some("2001:db8::1".to_string()));
                assert_eq!(old_remote_addr, Some("1.2.3.4:54321".to_string()));
                assert_eq!(new_remote_addr, Some("5.6.7.8:12345".to_string()));
            }
            _ => panic!("Expected ServerIpChanged"),
        }
    }

    #[test]
    fn test_server_upgrade_with_job_id_round_trip() {
        let msg = ServerMessage::Upgrade {
            version: "2.0.0".to_string(),
            download_url: "https://example.com/serverbee.tar.gz".to_string(),
            sha256: "abc123".to_string(),
            job_id: Some("job-1".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            ServerMessage::Upgrade {
                version,
                download_url,
                sha256,
                job_id,
            } => {
                assert_eq!(version, "2.0.0");
                assert_eq!(download_url, "https://example.com/serverbee.tar.gz");
                assert_eq!(sha256, "abc123");
                assert_eq!(job_id, Some("job-1".to_string()));
            }
            _ => panic!("Expected Upgrade"),
        }
    }

    #[test]
    fn test_upgrade_messages_without_job_id_stay_backward_compatible() {
        let server_json = r#"{"type":"upgrade","version":"2.0.0","download_url":"https://example.com/serverbee.tar.gz","sha256":"abc123"}"#;
        let server_msg: ServerMessage = serde_json::from_str(server_json).unwrap();
        match server_msg {
            ServerMessage::Upgrade {
                job_id,
                version,
                download_url,
                sha256,
            } => {
                assert_eq!(job_id, None);
                assert_eq!(version, "2.0.0");
                assert_eq!(download_url, "https://example.com/serverbee.tar.gz");
                assert_eq!(sha256, "abc123");
            }
            _ => panic!("Expected Upgrade"),
        }

        let agent_json = r#"{"type":"upgrade_progress","msg_id":"m1","target_version":"2.0.0","stage":"downloading"}"#;
        let agent_msg: AgentMessage = serde_json::from_str(agent_json).unwrap();
        match agent_msg {
            AgentMessage::UpgradeProgress {
                msg_id,
                job_id,
                target_version,
                stage,
            } => {
                assert_eq!(msg_id, "m1");
                assert_eq!(job_id, None);
                assert_eq!(target_version, "2.0.0");
                assert_eq!(stage, UpgradeStage::Downloading);
            }
            _ => panic!("Expected UpgradeProgress"),
        }
    }

    #[test]
    fn test_browser_full_sync_with_upgrades_round_trip() {
        let msg = BrowserMessage::FullSync {
            servers: vec![],
            upgrades: vec![UpgradeJobDto {
                server_id: "server-1".to_string(),
                job_id: "job-1".to_string(),
                target_version: "2.0.0".to_string(),
                stage: UpgradeStage::Installing,
                status: UpgradeStatus::Running,
                error: None,
                backup_path: Some("/backups/server-1.tar.gz".to_string()),
                started_at: chrono::Utc::now(),
                finished_at: None,
            }],
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            BrowserMessage::FullSync { servers, upgrades } => {
                assert!(servers.is_empty());
                assert_eq!(upgrades.len(), 1);
                assert_eq!(upgrades[0].server_id, "server-1");
                assert_eq!(upgrades[0].job_id, "job-1");
                assert_eq!(upgrades[0].target_version, "2.0.0");
                assert_eq!(upgrades[0].stage, UpgradeStage::Installing);
                assert_eq!(upgrades[0].status, UpgradeStatus::Running);
                assert_eq!(upgrades[0].error, None);
                assert_eq!(
                    upgrades[0].backup_path,
                    Some("/backups/server-1.tar.gz".to_string())
                );
                assert!(upgrades[0].finished_at.is_none());
            }
            _ => panic!("Expected FullSync"),
        }
    }

    #[test]
    fn test_agent_info_updated_accepts_optional_agent_version() {
        let json = r#"{"type":"agent_info_updated","server_id":"server-1","protocol_version":3,"agent_version":"1.2.3"}"#;

        match serde_json::from_str::<BrowserMessage>(json).unwrap() {
            BrowserMessage::AgentInfoUpdated {
                server_id,
                protocol_version,
                agent_version,
            } => {
                assert_eq!(server_id, "server-1");
                assert_eq!(protocol_version, 3);
                assert_eq!(agent_version.as_deref(), Some("1.2.3"));
            }
            _ => panic!("Expected AgentInfoUpdated"),
        }
    }
}
