use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TerminalAuditContext {
    pub server_id: String,
    pub user_id: String,
    pub ip: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DockerLogsAuditContext {
    pub server_id: String,
    pub user_id: String,
    pub ip: String,
    pub container_id: String,
    pub tail: Option<u64>,
    pub follow: bool,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExecAuditContext {
    pub user_id: String,
    pub ip: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DockerViewResource {
    Containers,
    Stats,
    Info,
    Events,
    Networks,
    Volumes,
}

impl DockerViewResource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Containers => "containers",
            Self::Stats => "stats",
            Self::Info => "info",
            Self::Events => "events",
            Self::Networks => "networks",
            Self::Volumes => "volumes",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_terminal_audit_context_serializes_to_json() {
        let context = TerminalAuditContext {
            server_id: "srv-1".to_string(),
            user_id: "user-1".to_string(),
            ip: "127.0.0.1".to_string(),
            started_at: Utc.with_ymd_and_hms(2026, 4, 12, 8, 30, 0).unwrap(),
        };

        let value = serde_json::to_value(context).expect("terminal context should serialize");

        assert_eq!(value["server_id"], "srv-1");
        assert_eq!(value["user_id"], "user-1");
        assert_eq!(value["ip"], "127.0.0.1");
        assert_eq!(value["started_at"], "2026-04-12T08:30:00Z");
    }

    #[test]
    fn test_docker_logs_audit_context_serializes_to_json() {
        let context = DockerLogsAuditContext {
            server_id: "srv-1".to_string(),
            user_id: "user-1".to_string(),
            ip: "127.0.0.1".to_string(),
            container_id: "container-1".to_string(),
            tail: Some(200),
            follow: true,
            started_at: Utc.with_ymd_and_hms(2026, 4, 12, 8, 30, 0).unwrap(),
        };

        let value = serde_json::to_value(context).expect("docker logs context should serialize");

        assert_eq!(value["container_id"], "container-1");
        assert_eq!(value["tail"], 200);
        assert_eq!(value["follow"], true);
    }

    #[test]
    fn test_docker_view_resource_serializes_stable_values() {
        assert_eq!(
            serde_json::to_string(&DockerViewResource::Containers)
                .expect("containers resource should serialize"),
            "\"containers\""
        );
        assert_eq!(DockerViewResource::Events.as_str(), "events");
        assert_eq!(DockerViewResource::Volumes.as_str(), "volumes");
    }
}
