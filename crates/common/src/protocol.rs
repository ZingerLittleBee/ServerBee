use serde::{Deserialize, Serialize};

use crate::types::{
    NetworkProbeResultData, NetworkProbeTarget, PingResult, PingTaskConfig, SystemInfo,
    SystemReport, TaskResult,
};

/// Agent -> Server messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    SystemInfo {
        msg_id: String,
        #[serde(flatten)]
        info: SystemInfo,
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
    },
    NetworkProbeResults {
        results: Vec<NetworkProbeResultData>,
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
    Ping,
    Upgrade {
        version: String,
        download_url: String,
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
    },
    AgentInfoUpdated {
        server_id: String,
        protocol_version: u32,
    },
    NetworkProbeUpdate {
        server_id: String,
        results: Vec<NetworkProbeResultData>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welcome_without_capabilities_deserializes() {
        let json = r#"{"type":"welcome","server_id":"s1","protocol_version":1,"report_interval":3}"#;
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
    fn test_capability_denied_round_trip() {
        let msg = AgentMessage::CapabilityDenied {
            msg_id: Some("task-1".to_string()),
            session_id: None,
            capability: "exec".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::CapabilityDenied {
                msg_id,
                session_id,
                capability,
            } => {
                assert_eq!(msg_id, Some("task-1".to_string()));
                assert_eq!(session_id, None);
                assert_eq!(capability, "exec");
            }
            _ => panic!("Expected CapabilityDenied"),
        }
    }

    #[test]
    fn test_system_info_without_protocol_version() {
        let json = r#"{"type":"system_info","msg_id":"m1","cpu_name":"Intel","cpu_cores":4,"cpu_arch":"x86_64","os":"Linux","kernel_version":"5.4","mem_total":8000000000,"swap_total":0,"disk_total":100000000000,"agent_version":"0.1.0"}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::SystemInfo { info, .. } => {
                assert_eq!(info.protocol_version, 1);
            }
            _ => panic!("Expected SystemInfo"),
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
            ServerMessage::NetworkProbeSync { interval, packet_count, .. } => {
                assert_eq!(interval, 60);
                assert_eq!(packet_count, 10);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_network_probe_results_serializes() {
        let msg = AgentMessage::NetworkProbeResults {
            results: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::NetworkProbeResults { results } => assert!(results.is_empty()),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_network_probe_result_with_null_latency() {
        use chrono::Utc;
        use crate::types::NetworkProbeResultData;
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
}
