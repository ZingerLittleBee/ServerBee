use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SecurityEventType {
    SshLogin,
    SshBruteForce,
    PortScan,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DetectorSource {
    Journal,
    AuthLog,
    Conntrack,
    FirewallLog,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SshAuthMethod {
    Publickey,
    Password,
    KeyboardInteractive,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecurityEvidence {
    SshLogin {
        auth_method: SshAuthMethod,
    },
    SshBruteForce {
        failed_count: u32,
        distinct_users: u32,
        sample_users: Vec<String>,
        invalid_user_count: u32,
        window_seconds: u32,
        threshold: u32,
    },
    PortScan {
        distinct_ports: u32,
        sample_ports: Vec<u16>,
        total_attempts: u32,
        window_seconds: u32,
        threshold: u32,
        blocked_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SecurityEventPayload {
    pub event_type: SecurityEventType,
    pub severity: Severity,
    pub source_ip: String,
    pub source_port: Option<u16>,
    pub username: Option<String>,
    pub started_at: i64, // unix seconds UTC
    pub ended_at: i64,
    pub first_seen: bool,
    pub detector_source: DetectorSource,
    pub evidence: SecurityEvidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trips() {
        let p = SecurityEventPayload {
            event_type: SecurityEventType::SshBruteForce,
            severity: Severity::High,
            source_ip: "203.0.113.5".into(),
            source_port: None,
            username: None,
            started_at: 1_700_000_000,
            ended_at: 1_700_000_060,
            first_seen: false,
            detector_source: DetectorSource::Journal,
            evidence: SecurityEvidence::SshBruteForce {
                failed_count: 47,
                distinct_users: 3,
                sample_users: vec!["root".into(), "admin".into()],
                invalid_user_count: 8,
                window_seconds: 60,
                threshold: 10,
            },
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: SecurityEventPayload = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.event_type, SecurityEventType::SshBruteForce));
        assert_eq!(back.source_ip, "203.0.113.5");
    }

    #[test]
    fn evidence_tag_serializes_to_kind() {
        let e = SecurityEvidence::SshLogin {
            auth_method: SshAuthMethod::Publickey,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "ssh_login");
    }
}
