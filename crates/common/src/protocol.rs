use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::constants::CapabilityDeniedReason;
use crate::docker_types::*;
use crate::types::{
    FileEntry, NetworkInterface, NetworkProbeResultData, NetworkProbeTarget, PingResult,
    PingTaskConfig, SystemInfo, SystemReport, TaskResult, TracerouteHop,
};

/// Strict input protocol enum used on `ServerMessage::Traceroute.protocol`
/// and on the server's POST request DTO. Only the three values the user can
/// pick are accepted; legacy is NOT part of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum TraceProtocol {
    Icmp,
    Udp,
    Tcp,
}

/// Persisted/read protocol enum. Extends `TraceProtocol` with `Legacy` for
/// records normalized from pre-trippy agents whose actual probe mode is
/// unknown (Unix `traceroute` defaults to UDP, `mtr` is ICMP, Windows
/// `tracert` is ICMP — the legacy agent does not report which ran).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum RecordedProtocol {
    Icmp,
    Udp,
    Tcp,
    Legacy,
}

impl From<TraceProtocol> for RecordedProtocol {
    fn from(p: TraceProtocol) -> Self {
        match p {
            TraceProtocol::Icmp => Self::Icmp,
            TraceProtocol::Udp => Self::Udp,
            TraceProtocol::Tcp => Self::Tcp,
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum RecoveryJobStatus {
    Running,
    Failed,
    Succeeded,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum RecoveryJobStage {
    Validating,
    Rebinding,
    AwaitingTargetOnline,
    FreezingWrites,
    MergingHistory,
    Finalizing,
    Succeeded,
    Failed,
    Unknown,
}

impl<'de> Deserialize<'de> for RecoveryJobStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        Ok(match value.as_str() {
            "running" => Self::Running,
            "failed" => Self::Failed,
            "succeeded" => Self::Succeeded,
            _ => Self::Unknown,
        })
    }
}

impl<'de> Deserialize<'de> for RecoveryJobStage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        Ok(match value.as_str() {
            "validating" => Self::Validating,
            "rebinding" => Self::Rebinding,
            "awaiting_target_online" => Self::AwaitingTargetOnline,
            "freezing_writes" => Self::FreezingWrites,
            "merging_history" => Self::MergingHistory,
            "finalizing" => Self::Finalizing,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            _ => Self::Unknown,
        })
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecoveryJobDto {
    pub job_id: String,
    pub target_server_id: String,
    pub source_server_id: String,
    pub status: RecoveryJobStatus,
    pub stage: RecoveryJobStage,
    #[serde(default)]
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
}

// --- IP Quality DTOs ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum UnlockStatus {
    Unlocked,
    Restricted,
    Blocked,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UnlockRequest {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum UnlockMatch {
    StatusEquals { code: u16 },
    StatusInRange { min: u16, max: u16 },
    BodyRegex { pattern: String },
    RedirectMatches { pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UnlockRule {
    #[serde(rename = "match")]
    pub match_: UnlockMatch,
    pub result: UnlockStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UnlockServiceDef {
    pub id: String,
    pub key: String,
    pub detector: Option<String>,
    pub request: Option<UnlockRequest>,
    pub rules: Option<Vec<UnlockRule>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UnlockResultData {
    pub service_id: String,
    pub status: UnlockStatus,
    pub region: Option<String>,
    pub latency_ms: Option<u32>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct IpQualitySnapshotData {
    pub ip: String,
    pub asn: Option<String>,
    pub as_org: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub ip_type: String,
    pub is_proxy: bool,
    pub is_vpn: bool,
    pub is_hosting: bool,
    pub risk_score: Option<i32>,
    pub risk_level: String,
    pub checked_at: DateTime<Utc>,
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
    SecurityEvent(crate::security::SecurityEventPayload),
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
    UnlockResults {
        egress_ip: String,
        results: Vec<UnlockResultData>,
        checked_at: DateTime<Utc>,
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
    /// Streamed by new (trippy-core) agents. One message per probe round.
    /// `hops` is the FULL accumulated state after this round, not a delta.
    /// `completed=true` marks the final update for `request_id`.
    TracerouteRoundUpdate {
        request_id: String,
        target: String,
        round: u32,
        total_rounds: u32,
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
    BlocklistAck {
        results: Vec<crate::firewall::BlocklistAckItem>,
    },
    BlocklistResetAck {
        ok: bool,
        reason: Option<String>,
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
    IpQualitySync {
        services: Vec<UnlockServiceDef>,
        interval_hours: u32,
    },
    IpQualityRunNow,
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
        /// Strict enum; defaults to ICMP behavior when missing for old-agent
        /// compatibility.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol: Option<TraceProtocol>,
    },
    Ping,
    /// Agent 自升级。`download_url`/`sha256` 自 pinned-source 版本起**废弃**:
    /// 新 Agent 忽略,仅 `version` 有效(来源由 Agent 本地配置决定)。
    Upgrade {
        version: String,
        #[serde(default)]
        download_url: String,
        #[serde(default)]
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
    /// Full-state sync. Agent reconciles its nft set diff against this list
    /// and emits one BlocklistAck item per entry it touched.
    BlocklistSync {
        entries: Vec<crate::firewall::BlockEntry>,
    },
    /// Incremental add. Agent applies, then emits a single-item BlocklistAck.
    BlocklistAdd {
        entry: crate::firewall::BlockEntry,
    },
    /// Incremental remove. Agent applies, then emits a single-item BlocklistAck.
    BlocklistRemove {
        id: String,
    },
    /// Unconditional wipe of the agent's firewall state. Honored regardless
    /// of capability bit; intended for capability-revoke cleanup.
    BlocklistReset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEventBroadcast {
    pub server_id: String,
    pub event_id: String,
    pub event: crate::security::SecurityEventPayload,
}

/// Server -> Browser messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserMessage {
    FullSync {
        servers: Vec<crate::types::ServerStatus>,
        #[serde(default)]
        upgrades: Vec<UpgradeJobDto>,
        #[serde(default)]
        recoveries: Vec<RecoveryJobDto>,
    },
    Update {
        servers: Vec<crate::types::ServerStatus>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recoveries: Option<Vec<RecoveryJobDto>>,
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
    SecurityEvent(SecurityEventBroadcast),
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
    TracerouteUpdate {
        server_id: String,
        request_id: String,
        target: String,
        /// From the server-side TracerouteRequestMeta cache so any browser
        /// (not only the originator) and reconnecting clients render the
        /// correct label without an extra GET round-trip.
        protocol: RecordedProtocol,
        started_at: i64,
        round: u32,
        total_rounds: u32,
        /// Server-side enriched (hostname filled in; ASN deferred).
        hops: Vec<TracerouteHop>,
        completed: bool,
        error: Option<String>,
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
    BlocklistChanged {
        kind: crate::firewall::BlocklistChangeKind,
        block_id: String,
        target: String,
    },
    FirewallApplyStateChanged {
        block_id: String,
        server_id: String,
        state: crate::firewall::BlocklistEntryState,
        reason: Option<String>,
    },
    IpQualityUpdate {
        server_id: String,
        unlock_results: Vec<UnlockResultData>,
        ip_quality: Option<IpQualitySnapshotData>,
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
            protocol: None,
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
                ..
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
                    ips: vec![], total_sent: None, total_recv: None,
                    loss_pct: None, best_ms: None, worst_ms: None, avg_ms: None,
                    stddev_ms: None, jitter_ms: None,
                },
                TracerouteHop {
                    hop: 2,
                    ip: None,
                    hostname: None,
                    rtt1: None,
                    rtt2: None,
                    rtt3: None,
                    asn: None,
                    ips: vec![], total_sent: None, total_recv: None,
                    loss_pct: None, best_ms: None, worst_ms: None, avg_ms: None,
                    stddev_ms: None, jitter_ms: None,
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
    fn test_recovery_job_dto_round_trip() {
        let dto = RecoveryJobDto {
            job_id: "recovery-1".to_string(),
            target_server_id: "target-1".to_string(),
            source_server_id: "source-1".to_string(),
            status: RecoveryJobStatus::Running,
            stage: RecoveryJobStage::FreezingWrites,
            error: Some("write freeze in progress".to_string()),
            started_at: chrono::DateTime::parse_from_rfc3339("2026-04-16T01:02:03Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            created_at: chrono::DateTime::parse_from_rfc3339("2026-04-16T01:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339("2026-04-16T01:05:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            last_heartbeat_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-04-16T01:04:30Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
        };

        let json = serde_json::to_string(&dto).unwrap();
        let parsed: RecoveryJobDto = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, dto);
    }

    #[test]
    fn test_recovery_job_status_unknown_deserializes_to_unknown() {
        let status: RecoveryJobStatus = serde_json::from_str(r#""paused""#).unwrap();

        assert_eq!(status, RecoveryJobStatus::Unknown);
    }

    #[test]
    fn test_recovery_job_stage_unknown_deserializes_to_unknown() {
        let stage: RecoveryJobStage = serde_json::from_str(r#""reconciling""#).unwrap();

        assert_eq!(stage, RecoveryJobStage::Unknown);
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
            recoveries: vec![RecoveryJobDto {
                job_id: "recovery-1".to_string(),
                target_server_id: "target-1".to_string(),
                source_server_id: "source-1".to_string(),
                status: RecoveryJobStatus::Running,
                stage: RecoveryJobStage::Rebinding,
                error: Some("waiting for agent reconnect".to_string()),
                started_at: chrono::DateTime::parse_from_rfc3339("2026-04-16T01:02:03Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                created_at: chrono::DateTime::parse_from_rfc3339("2026-04-16T01:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339("2026-04-16T01:05:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                last_heartbeat_at: Some(
                    chrono::DateTime::parse_from_rfc3339("2026-04-16T01:04:30Z")
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                ),
            }],
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            BrowserMessage::FullSync {
                servers,
                upgrades,
                recoveries,
            } => {
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
                assert_eq!(recoveries.len(), 1);
                assert_eq!(recoveries[0].job_id, "recovery-1");
                assert_eq!(recoveries[0].target_server_id, "target-1");
                assert_eq!(recoveries[0].source_server_id, "source-1");
                assert_eq!(recoveries[0].status, RecoveryJobStatus::Running);
                assert_eq!(recoveries[0].stage, RecoveryJobStage::Rebinding);
                assert_eq!(
                    recoveries[0].error,
                    Some("waiting for agent reconnect".to_string())
                );
                assert_eq!(
                    recoveries[0].started_at,
                    chrono::DateTime::parse_from_rfc3339("2026-04-16T01:02:03Z")
                        .unwrap()
                        .with_timezone(&chrono::Utc)
                );
                assert_eq!(
                    recoveries[0].created_at,
                    chrono::DateTime::parse_from_rfc3339("2026-04-16T01:00:00Z")
                        .unwrap()
                        .with_timezone(&chrono::Utc)
                );
                assert_eq!(
                    recoveries[0].updated_at,
                    chrono::DateTime::parse_from_rfc3339("2026-04-16T01:05:00Z")
                        .unwrap()
                        .with_timezone(&chrono::Utc)
                );
                assert_eq!(
                    recoveries[0].last_heartbeat_at,
                    Some(
                        chrono::DateTime::parse_from_rfc3339("2026-04-16T01:04:30Z")
                            .unwrap()
                            .with_timezone(&chrono::Utc)
                    )
                );
            }
            _ => panic!("Expected FullSync"),
        }
    }

    #[test]
    fn test_browser_full_sync_defaults_missing_recoveries_to_empty() {
        let json = r#"{"type":"full_sync","servers":[],"upgrades":[]}"#;
        let parsed: BrowserMessage = serde_json::from_str(json).unwrap();

        match parsed {
            BrowserMessage::FullSync {
                servers,
                upgrades,
                recoveries,
            } => {
                assert!(servers.is_empty());
                assert!(upgrades.is_empty());
                assert!(recoveries.is_empty());
            }
            _ => panic!("Expected FullSync"),
        }
    }

    #[test]
    fn test_browser_update_omits_recoveries_when_none() {
        let msg = BrowserMessage::Update {
            servers: vec![],
            recoveries: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "update");
        assert_eq!(value["servers"], serde_json::json!([]));
        assert!(value.get("recoveries").is_none());

        match serde_json::from_str::<BrowserMessage>(&json).unwrap() {
            BrowserMessage::Update {
                servers,
                recoveries,
            } => {
                assert!(servers.is_empty());
                assert!(recoveries.is_none());
            }
            _ => panic!("Expected Update"),
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

    #[test]
    fn server_message_blocklist_reset_encodes() {
        let json = serde_json::to_string(&ServerMessage::BlocklistReset).unwrap();
        // Internal-tagged enum with snake_case: {"type":"blocklist_reset"}
        assert_eq!(json, r#"{"type":"blocklist_reset"}"#);
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ServerMessage::BlocklistReset));
    }

    #[test]
    fn server_message_blocklist_sync_round_trip() {
        use crate::firewall::BlockEntry;
        let msg = ServerMessage::BlocklistSync {
            entries: vec![BlockEntry {
                id: "b1".into(),
                target: "1.2.3.4/32".into(),
                family: 4,
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"blocklist_sync\""));
        assert!(json.contains("\"target\":\"1.2.3.4/32\""));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::BlocklistSync { entries } => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].id, "b1");
            }
            _ => panic!("Expected BlocklistSync"),
        }
    }

    #[test]
    fn agent_message_blocklist_ack_encodes() {
        use crate::firewall::{BlocklistAckItem, BlocklistEntryState};
        let msg = AgentMessage::BlocklistAck {
            results: vec![BlocklistAckItem {
                id: "b1".into(),
                state: BlocklistEntryState::Present,
                reason: None,
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"blocklist_ack\""));
        assert!(json.contains("\"state\":\"present\""));
        let _: AgentMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn browser_message_blocklist_changed_encodes() {
        use crate::firewall::BlocklistChangeKind;
        let msg = BrowserMessage::BlocklistChanged {
            kind: BlocklistChangeKind::Created,
            block_id: "b1".into(),
            target: "1.2.3.4/32".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"blocklist_changed\""));
        assert!(json.contains("\"kind\":\"created\""));
        let _: BrowserMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_upgrade_deserializes_without_deprecated_fields() {
        let json = r#"{"type":"upgrade","version":"1.0.0"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Upgrade {
                version,
                download_url,
                sha256,
                job_id,
            } => {
                assert_eq!(version, "1.0.0");
                assert_eq!(download_url, "");
                assert_eq!(sha256, "");
                assert_eq!(job_id, None);
            }
            _ => panic!("expected Upgrade"),
        }
    }

    #[test]
    fn ip_quality_sync_round_trip() {
        let msg = ServerMessage::IpQualitySync {
            services: vec![UnlockServiceDef {
                id: "svc-1".to_string(),
                key: "custom-site".to_string(),
                detector: None,
                request: Some(UnlockRequest {
                    url: "https://example.com/check".to_string(),
                    method: "GET".to_string(),
                    headers: vec![("User-Agent".to_string(), "serverbee".to_string())],
                    timeout_ms: 5000,
                }),
                rules: Some(vec![UnlockRule {
                    match_: UnlockMatch::StatusEquals { code: 200 },
                    result: UnlockStatus::Unlocked,
                }]),
            }],
            interval_hours: 12,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ip_quality_sync\""));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::IpQualitySync {
                services,
                interval_hours,
            } => {
                assert_eq!(interval_hours, 12);
                assert_eq!(services.len(), 1);
                assert_eq!(services[0].id, "svc-1");
                assert_eq!(services[0].key, "custom-site");
                assert!(services[0].detector.is_none());
                let request = services[0].request.as_ref().unwrap();
                assert_eq!(request.url, "https://example.com/check");
                assert_eq!(request.method, "GET");
                assert_eq!(request.headers.len(), 1);
                assert_eq!(request.timeout_ms, 5000);
                let rules = services[0].rules.as_ref().unwrap();
                assert_eq!(rules.len(), 1);
                assert!(matches!(
                    rules[0].match_,
                    UnlockMatch::StatusEquals { code: 200 }
                ));
                assert_eq!(rules[0].result, UnlockStatus::Unlocked);
            }
            _ => panic!("Expected IpQualitySync"),
        }
    }

    #[test]
    fn ip_quality_run_now_encodes() {
        let json = serde_json::to_string(&ServerMessage::IpQualityRunNow).unwrap();
        assert_eq!(json, r#"{"type":"ip_quality_run_now"}"#);
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ServerMessage::IpQualityRunNow));
    }

    #[test]
    fn unlock_results_round_trip() {
        let checked_at = chrono::DateTime::parse_from_rfc3339("2026-05-22T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let msg = AgentMessage::UnlockResults {
            egress_ip: "203.0.113.7".to_string(),
            results: vec![UnlockResultData {
                service_id: "svc-1".to_string(),
                status: UnlockStatus::Restricted,
                region: Some("US".to_string()),
                latency_ms: Some(123),
                detail: Some("originals only".to_string()),
            }],
            checked_at,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"unlock_results\""));
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::UnlockResults {
                egress_ip,
                results,
                checked_at: parsed_checked_at,
            } => {
                assert_eq!(egress_ip, "203.0.113.7");
                assert_eq!(parsed_checked_at, checked_at);
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].service_id, "svc-1");
                assert_eq!(results[0].status, UnlockStatus::Restricted);
                assert_eq!(results[0].region, Some("US".to_string()));
                assert_eq!(results[0].latency_ms, Some(123));
                assert_eq!(results[0].detail, Some("originals only".to_string()));
            }
            _ => panic!("Expected UnlockResults"),
        }
    }

    #[test]
    fn browser_ip_quality_update_round_trip() {
        // Form 1: partial update, ip_quality = None.
        let partial = BrowserMessage::IpQualityUpdate {
            server_id: "srv-1".to_string(),
            unlock_results: vec![UnlockResultData {
                service_id: "svc-1".to_string(),
                status: UnlockStatus::Unlocked,
                region: None,
                latency_ms: Some(88),
                detail: None,
            }],
            ip_quality: None,
        };
        let json = serde_json::to_string(&partial).unwrap();
        assert!(json.contains("\"type\":\"ip_quality_update\""));
        let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            BrowserMessage::IpQualityUpdate {
                server_id,
                unlock_results,
                ip_quality,
            } => {
                assert_eq!(server_id, "srv-1");
                assert_eq!(unlock_results.len(), 1);
                assert_eq!(unlock_results[0].status, UnlockStatus::Unlocked);
                assert!(ip_quality.is_none());
            }
            _ => panic!("Expected IpQualityUpdate"),
        }

        // Form 2: full update, ip_quality = Some(..).
        let checked_at = chrono::DateTime::parse_from_rfc3339("2026-05-22T10:05:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let full = BrowserMessage::IpQualityUpdate {
            server_id: "srv-1".to_string(),
            unlock_results: vec![],
            ip_quality: Some(IpQualitySnapshotData {
                ip: "203.0.113.7".to_string(),
                asn: Some("AS64500".to_string()),
                as_org: Some("Example Hosting".to_string()),
                country: Some("US".to_string()),
                region: Some("CA".to_string()),
                city: Some("San Jose".to_string()),
                ip_type: "datacenter".to_string(),
                is_proxy: false,
                is_vpn: false,
                is_hosting: true,
                risk_score: Some(42),
                risk_level: "medium".to_string(),
                checked_at,
            }),
        };
        let json = serde_json::to_string(&full).unwrap();
        let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            BrowserMessage::IpQualityUpdate {
                server_id,
                unlock_results,
                ip_quality,
            } => {
                assert_eq!(server_id, "srv-1");
                assert!(unlock_results.is_empty());
                let snapshot = ip_quality.expect("ip_quality should be Some");
                assert_eq!(snapshot.ip, "203.0.113.7");
                assert_eq!(snapshot.asn, Some("AS64500".to_string()));
                assert_eq!(snapshot.as_org, Some("Example Hosting".to_string()));
                assert_eq!(snapshot.ip_type, "datacenter");
                assert!(!snapshot.is_proxy);
                assert!(snapshot.is_hosting);
                assert_eq!(snapshot.risk_score, Some(42));
                assert_eq!(snapshot.risk_level, "medium");
                assert_eq!(snapshot.checked_at, checked_at);
            }
            _ => panic!("Expected IpQualityUpdate"),
        }
    }

    #[test]
    fn test_browser_message_traceroute_update_round_trip() {
        use crate::types::TracerouteHop;
        let msg = BrowserMessage::TracerouteUpdate {
            server_id: "srv-1".into(),
            request_id: "rid-5".into(),
            target: "1.1.1.1".into(),
            protocol: RecordedProtocol::Tcp,
            started_at: 1_716_500_000_000,
            round: 1,
            total_rounds: 5,
            hops: vec![TracerouteHop {
                hop: 1, ip: None, hostname: Some("hop1.example".into()),
                rtt1: None, rtt2: None, rtt3: None, asn: None,
                ips: vec!["10.0.0.1".into()],
                total_sent: Some(1), total_recv: Some(1),
                loss_pct: Some(0.0),
                best_ms: Some(1.0), worst_ms: Some(1.0), avg_ms: Some(1.0),
                stddev_ms: Some(0.0), jitter_ms: Some(0.0),
            }],
            completed: false,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"traceroute_update\""));
        assert!(json.contains("\"protocol\":\"tcp\""));
        let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            BrowserMessage::TracerouteUpdate { protocol, started_at, .. } => {
                assert_eq!(protocol, RecordedProtocol::Tcp);
                assert_eq!(started_at, 1_716_500_000_000);
            }
            _ => panic!("Expected TracerouteUpdate"),
        }
    }

    #[test]
    fn test_traceroute_round_update_round_trip_intermediate() {
        use crate::types::TracerouteHop;
        let msg = AgentMessage::TracerouteRoundUpdate {
            request_id: "rid-3".into(),
            target: "1.1.1.1".into(),
            round: 2,
            total_rounds: 5,
            hops: vec![TracerouteHop {
                hop: 1, ip: None, hostname: None,
                rtt1: None, rtt2: None, rtt3: None, asn: None,
                ips: vec!["10.0.0.1".into()],
                total_sent: Some(2), total_recv: Some(2),
                loss_pct: Some(0.0),
                best_ms: Some(1.0), worst_ms: Some(1.2), avg_ms: Some(1.1),
                stddev_ms: Some(0.1), jitter_ms: Some(0.05),
            }],
            completed: false,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"traceroute_round_update\""));
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::TracerouteRoundUpdate { round, total_rounds, completed, hops, .. } => {
                assert_eq!(round, 2);
                assert_eq!(total_rounds, 5);
                assert!(!completed);
                assert_eq!(hops.len(), 1);
            }
            _ => panic!("Expected TracerouteRoundUpdate"),
        }
    }

    #[test]
    fn test_traceroute_round_update_terminal_error() {
        let msg = AgentMessage::TracerouteRoundUpdate {
            request_id: "rid-4".into(),
            target: "1.1.1.1".into(),
            round: 0,
            total_rounds: 0,
            hops: vec![],
            completed: true,
            error: Some("Traceroute requires elevated privileges".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::TracerouteRoundUpdate { completed, error, .. } => {
                assert!(completed);
                assert!(error.as_deref().unwrap().contains("privileges"));
            }
            _ => panic!("Expected TracerouteRoundUpdate"),
        }
    }

    #[test]
    fn test_traceroute_server_message_with_protocol_round_trip() {
        let msg = ServerMessage::Traceroute {
            request_id: "rid-1".into(),
            target: "1.1.1.1".into(),
            max_hops: 30,
            protocol: Some(TraceProtocol::Udp),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"protocol\":\"udp\""));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::Traceroute { protocol, .. } => assert_eq!(protocol, Some(TraceProtocol::Udp)),
            _ => panic!("Expected Traceroute"),
        }
    }

    #[test]
    fn test_traceroute_server_message_protocol_omitted_when_none() {
        // Old agents will see absent key and default to ICMP via existing behavior.
        let msg = ServerMessage::Traceroute {
            request_id: "rid-2".into(),
            target: "8.8.8.8".into(),
            max_hops: 30,
            protocol: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("\"protocol\""), "got: {json}");
    }

    #[test]
    fn test_traceroute_hop_legacy_fields_skipped_when_none() {
        // A new-schema hop (filled by trippy) should NOT carry stale rtt1/2/3
        // keys in its JSON. Round-trip a hop with new fields populated but
        // legacy fields None and assert the serialized form has no rtt* / ip
        // keys (they are skip_serializing_if Option::is_none).
        let hop = TracerouteHop {
            hop: 1,
            ip: None,
            hostname: Some("router.local".into()),
            rtt1: None, rtt2: None, rtt3: None,
            asn: None,
            ips: vec!["10.0.0.1".into()],
            total_sent: Some(5),
            total_recv: Some(5),
            loss_pct: Some(0.0),
            best_ms: Some(1.1),
            worst_ms: Some(1.5),
            avg_ms: Some(1.3),
            stddev_ms: Some(0.15),
            jitter_ms: Some(0.05),
        };
        let json = serde_json::to_string(&hop).unwrap();
        assert!(!json.contains("\"rtt1\""), "got: {json}");
        assert!(!json.contains("\"ip\":"), "got: {json}");
        assert!(json.contains("\"ips\":[\"10.0.0.1\"]"));
        assert!(json.contains("\"loss_pct\":0.0"));
    }

    #[test]
    fn test_traceroute_hop_new_schema_fields_skipped_when_default() {
        // A legacy-schema hop emitted by an old agent should NOT carry empty
        // ips: [] or null new-schema fields in JSON. Round-trip a legacy hop
        // and assert ips / total_sent etc. are absent.
        let hop = TracerouteHop {
            hop: 2,
            ip: Some("8.8.8.8".into()),
            hostname: Some("dns.google".into()),
            rtt1: Some(12.0), rtt2: Some(11.8), rtt3: Some(12.3),
            asn: Some("AS15169".into()),
            ips: vec![],
            total_sent: None, total_recv: None,
            loss_pct: None,
            best_ms: None, worst_ms: None, avg_ms: None,
            stddev_ms: None, jitter_ms: None,
        };
        let json = serde_json::to_string(&hop).unwrap();
        assert!(!json.contains("\"ips\":"),       "got: {json}");
        assert!(!json.contains("\"total_sent\""), "got: {json}");
        assert!(!json.contains("\"loss_pct\""),   "got: {json}");
        assert!(json.contains("\"rtt1\":12.0"));
    }

    #[test]
    fn test_trace_protocol_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&TraceProtocol::Icmp).unwrap(), "\"icmp\"");
        assert_eq!(serde_json::to_string(&TraceProtocol::Udp).unwrap(), "\"udp\"");
        assert_eq!(serde_json::to_string(&TraceProtocol::Tcp).unwrap(), "\"tcp\"");
    }

    #[test]
    fn test_trace_protocol_rejects_unknown_value() {
        let err = serde_json::from_str::<TraceProtocol>("\"banana\"").unwrap_err();
        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn test_trace_protocol_rejects_legacy_value() {
        // Legacy is a DB/read sentinel, not a probe-mode value the agent accepts.
        assert!(serde_json::from_str::<TraceProtocol>("\"legacy\"").is_err());
    }

    #[test]
    fn test_recorded_protocol_serializes_lowercase_including_legacy() {
        assert_eq!(serde_json::to_string(&RecordedProtocol::Icmp).unwrap(), "\"icmp\"");
        assert_eq!(serde_json::to_string(&RecordedProtocol::Legacy).unwrap(), "\"legacy\"");
    }

    #[test]
    fn test_recorded_protocol_from_trace_protocol() {
        assert_eq!(RecordedProtocol::from(TraceProtocol::Icmp), RecordedProtocol::Icmp);
        assert_eq!(RecordedProtocol::from(TraceProtocol::Udp), RecordedProtocol::Udp);
        assert_eq!(RecordedProtocol::from(TraceProtocol::Tcp), RecordedProtocol::Tcp);
    }
}
