use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub created: i64,
    pub ports: Vec<DockerPort>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerPort {
    pub private_port: u16,
    pub public_port: Option<u16>,
    pub port_type: String,
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerContainerStats {
    pub id: String,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub memory_percent: f64,
    pub network_rx: u64,
    pub network_tx: u64,
    pub block_read: u64,
    pub block_write: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerLogEntry {
    pub timestamp: Option<String>,
    pub stream: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerEventInfo {
    pub timestamp: i64,
    pub event_type: String,
    pub action: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerSystemInfo {
    pub docker_version: String,
    pub api_version: String,
    pub os: String,
    pub arch: String,
    pub containers_running: i64,
    pub containers_paused: i64,
    pub containers_stopped: i64,
    pub images: i64,
    pub memory_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerNetwork {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub containers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerVolume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: Option<String>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DockerAction {
    Start,
    Stop { timeout: Option<i64> },
    Restart { timeout: Option<i64> },
    Remove { force: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_container_serde() {
        let container = DockerContainer {
            id: "abc123".into(),
            name: "nginx".into(),
            image: "nginx:alpine".into(),
            state: "running".into(),
            status: "Up 3 hours".into(),
            created: 1710000000,
            ports: vec![DockerPort {
                private_port: 80,
                public_port: Some(8080),
                port_type: "tcp".into(),
                ip: Some("0.0.0.0".into()),
            }],
            labels: HashMap::new(),
        };
        let json = serde_json::to_string(&container).unwrap();
        let deserialized: DockerContainer = serde_json::from_str(&json).unwrap();
        assert_eq!(container, deserialized);
    }

    #[test]
    fn test_docker_action_serde() {
        let action = DockerAction::Stop { timeout: Some(10) };
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: DockerAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_docker_log_entry_serde() {
        let entry = DockerLogEntry {
            timestamp: Some("2026-03-18T10:00:00Z".into()),
            stream: "stdout".into(),
            message: "Server started".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: DockerLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }
}
