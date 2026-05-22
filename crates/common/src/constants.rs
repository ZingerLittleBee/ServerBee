pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_SERVER_PORT: u16 = 9527;
pub const DEFAULT_REPORT_INTERVAL: u32 = 3;
pub const PROTOCOL_VERSION: u32 = 4;

pub const SESSION_TTL_SECS: i64 = 86400;
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;
pub const OFFLINE_THRESHOLD_SECS: u64 = 30;

pub const MAX_WS_MESSAGE_SIZE: usize = 1024 * 1024;
pub const MAX_TASK_OUTPUT_SIZE: usize = 512 * 1024;
pub const MAX_BINARY_FRAME_SIZE: usize = 64 * 1024;
pub const MAX_FILE_CHUNK_SIZE: usize = 384 * 1024;
pub const MAX_FILE_CONCURRENT_TRANSFERS: usize = 3;
pub const FILE_TRANSFER_TIMEOUT_SECS: u64 = 1800;
pub const MAX_COMMAND_SIZE: usize = 8 * 1024;
pub const MAX_CONCURRENT_COMMANDS: usize = 5;
pub const MAX_TERMINAL_SESSIONS: usize = 3;
pub const TERMINAL_IDLE_TIMEOUT_SECS: u64 = 600;
pub const DEFAULT_COMMAND_TIMEOUT_SECS: u32 = 300;

pub const RECORDS_RETENTION_DAYS: u32 = 7;
pub const RECORDS_HOURLY_RETENTION_DAYS: u32 = 90;
pub const GPU_RECORDS_RETENTION_DAYS: u32 = 7;
pub const PING_RECORDS_RETENTION_DAYS: u32 = 7;
pub const AUDIT_LOGS_RETENTION_DAYS: u32 = 180;

pub const ALERT_DEBOUNCE_SECS: u64 = 300;
pub const ALERT_SAMPLE_MINUTES: u32 = 10;
pub const ALERT_TRIGGER_RATIO: f64 = 0.7;

pub const API_KEY_PREFIX: &str = "serverbee_";
pub const API_KEY_PREFIX_LEN: usize = 8;

// --- Capability Toggles ---

pub const CAP_TERMINAL: u32 = 1 << 0; // 1
pub const CAP_EXEC: u32 = 1 << 1; // 2
pub const CAP_UPGRADE: u32 = 1 << 2; // 4
pub const CAP_PING_ICMP: u32 = 1 << 3; // 8
pub const CAP_PING_TCP: u32 = 1 << 4; // 16
pub const CAP_PING_HTTP: u32 = 1 << 5; // 32
pub const CAP_FILE: u32 = 1 << 6; // 64
pub const CAP_DOCKER: u32 = 1 << 7; // 128
pub const CAP_SECURITY_EVENTS: u32 = 1 << 8; // 256
pub const CAP_FIREWALL_BLOCK: u32 = 1 << 9; // 512
pub const CAP_IP_QUALITY: u32 = 1 << 10; // 1024

pub const CAP_DEFAULT: u32 =
    CAP_UPGRADE | CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP | CAP_SECURITY_EVENTS; // 316 — firewall NOT in default
pub const CAP_VALID_MASK: u32 = 0b111_1111_1111; // 2047 — bits 0..=10

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityKey {
    Terminal,
    Exec,
    Upgrade,
    PingIcmp,
    PingTcp,
    PingHttp,
    File,
    Docker,
    SecurityEvents,
    FirewallBlock,
    IpQuality,
}

impl CapabilityKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Exec => "exec",
            Self::Upgrade => "upgrade",
            Self::PingIcmp => "ping_icmp",
            Self::PingTcp => "ping_tcp",
            Self::PingHttp => "ping_http",
            Self::File => "file",
            Self::Docker => "docker",
            Self::SecurityEvents => "security_events",
            Self::FirewallBlock => "firewall_block",
            Self::IpQuality => "ip_quality",
        }
    }

    pub fn to_bit(self) -> u32 {
        match self {
            Self::Terminal => CAP_TERMINAL,
            Self::Exec => CAP_EXEC,
            Self::Upgrade => CAP_UPGRADE,
            Self::PingIcmp => CAP_PING_ICMP,
            Self::PingTcp => CAP_PING_TCP,
            Self::PingHttp => CAP_PING_HTTP,
            Self::File => CAP_FILE,
            Self::Docker => CAP_DOCKER,
            Self::SecurityEvents => CAP_SECURITY_EVENTS,
            Self::FirewallBlock => CAP_FIREWALL_BLOCK,
            Self::IpQuality => CAP_IP_QUALITY,
        }
    }
}

impl std::str::FromStr for CapabilityKey {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "terminal" => Ok(Self::Terminal),
            "exec" => Ok(Self::Exec),
            "upgrade" => Ok(Self::Upgrade),
            "ping_icmp" => Ok(Self::PingIcmp),
            "ping_tcp" => Ok(Self::PingTcp),
            "ping_http" => Ok(Self::PingHttp),
            "file" => Ok(Self::File),
            "docker" => Ok(Self::Docker),
            "security_events" => Ok(Self::SecurityEvents),
            "firewall_block" => Ok(Self::FirewallBlock),
            "ip_quality" => Ok(Self::IpQuality),
            _ => Err(format!("unknown capability: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDeniedReason {
    ServerCapabilityDisabled,
    AgentCapabilityDisabled,
}

pub fn effective_capabilities(server_caps: u32, agent_local_caps: u32) -> u32 {
    server_caps & agent_local_caps
}

#[derive(Debug)]
pub struct CapabilityMeta {
    pub bit: u32,
    pub key: &'static str,
    pub display_name: &'static str,
    pub default_enabled: bool,
    pub risk_level: &'static str,
}

pub const ALL_CAPABILITIES: &[CapabilityMeta] = &[
    CapabilityMeta {
        bit: CAP_TERMINAL,
        key: "terminal",
        display_name: "Web Terminal",
        default_enabled: false,
        risk_level: "high",
    },
    CapabilityMeta {
        bit: CAP_EXEC,
        key: "exec",
        display_name: "Remote Exec",
        default_enabled: false,
        risk_level: "high",
    },
    CapabilityMeta {
        bit: CAP_UPGRADE,
        key: "upgrade",
        display_name: "Auto Upgrade",
        default_enabled: true,
        risk_level: "low",
    },
    CapabilityMeta {
        bit: CAP_PING_ICMP,
        key: "ping_icmp",
        display_name: "ICMP Ping",
        default_enabled: true,
        risk_level: "low",
    },
    CapabilityMeta {
        bit: CAP_PING_TCP,
        key: "ping_tcp",
        display_name: "TCP Probe",
        default_enabled: true,
        risk_level: "low",
    },
    CapabilityMeta {
        bit: CAP_PING_HTTP,
        key: "ping_http",
        display_name: "HTTP Probe",
        default_enabled: true,
        risk_level: "low",
    },
    CapabilityMeta {
        bit: CAP_FILE,
        key: "file",
        display_name: "File Manager",
        default_enabled: false,
        risk_level: "high",
    },
    CapabilityMeta {
        bit: CAP_DOCKER,
        key: "docker",
        display_name: "Docker Management",
        default_enabled: false,
        risk_level: "high",
    },
    CapabilityMeta {
        bit: CAP_SECURITY_EVENTS,
        key: "security_events",
        display_name: "Security Events",
        default_enabled: true,
        risk_level: "low",
    },
    CapabilityMeta {
        bit: CAP_FIREWALL_BLOCK,
        key: "firewall_block",
        display_name: "Firewall Blocklist",
        default_enabled: false,
        risk_level: "high",
    },
    CapabilityMeta {
        bit: CAP_IP_QUALITY,
        key: "ip_quality",
        display_name: "IP Quality",
        default_enabled: false,
        risk_level: "medium",
    },
];

/// Check if a specific capability bit is set.
pub fn has_capability(capabilities: u32, cap_bit: u32) -> bool {
    capabilities & cap_bit != 0
}

/// Map probe_type string to capability bit.
pub fn probe_type_to_cap(probe_type: &str) -> Option<u32> {
    match probe_type {
        "icmp" => Some(CAP_PING_ICMP),
        "tcp" => Some(CAP_PING_TCP),
        "http" => Some(CAP_PING_HTTP),
        _ => None,
    }
}

#[cfg(test)]
#[test]
fn protocol_version() {
    assert_eq!(PROTOCOL_VERSION, 4);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_capability_single_bit() {
        assert!(has_capability(CAP_TERMINAL, CAP_TERMINAL));
        assert!(!has_capability(0, CAP_TERMINAL));
        assert!(!has_capability(CAP_EXEC, CAP_TERMINAL));
    }

    #[test]
    fn test_has_capability_combined() {
        let caps = CAP_TERMINAL | CAP_EXEC;
        assert!(has_capability(caps, CAP_TERMINAL));
        assert!(has_capability(caps, CAP_EXEC));
        assert!(!has_capability(caps, CAP_UPGRADE));
    }

    #[test]
    fn test_default_capabilities() {
        assert!(!has_capability(CAP_DEFAULT, CAP_TERMINAL));
        assert!(!has_capability(CAP_DEFAULT, CAP_EXEC));
        assert!(has_capability(CAP_DEFAULT, CAP_UPGRADE));
        assert!(has_capability(CAP_DEFAULT, CAP_PING_ICMP));
        assert!(has_capability(CAP_DEFAULT, CAP_PING_TCP));
        assert!(has_capability(CAP_DEFAULT, CAP_PING_HTTP));
    }

    #[test]
    fn test_upgrade_metadata_is_low_risk_and_enabled_by_default() {
        let upgrade = ALL_CAPABILITIES
            .iter()
            .find(|meta| meta.key == "upgrade")
            .expect("upgrade capability should exist");

        assert!(upgrade.default_enabled);
        assert_eq!(upgrade.risk_level, "low");
    }

    #[test]
    fn test_valid_mask() {
        assert_eq!(CAP_VALID_MASK, 2047);
        for meta in ALL_CAPABILITIES {
            assert!(meta.bit & CAP_VALID_MASK == meta.bit);
        }
        let invalid_bit = 1 << ALL_CAPABILITIES.len();
        assert_ne!(invalid_bit & !CAP_VALID_MASK, 0);
    }

    #[test]
    fn cap_firewall_block_bit() {
        assert_eq!(CAP_FIREWALL_BLOCK, 512);
        assert_eq!(CAP_VALID_MASK & CAP_FIREWALL_BLOCK, CAP_FIREWALL_BLOCK);
        assert_eq!(CAP_DEFAULT & CAP_FIREWALL_BLOCK, 0); // not in default
    }

    #[test]
    fn cap_default_includes_security_events() {
        assert!(has_capability(CAP_DEFAULT, CAP_SECURITY_EVENTS));
        assert_eq!(CAP_DEFAULT, 316);
    }

    #[test]
    fn cap_valid_mask_covers_new_bit() {
        assert_eq!(CAP_VALID_MASK & CAP_SECURITY_EVENTS, CAP_SECURITY_EVENTS);
    }

    #[test]
    fn all_capabilities_includes_security_events() {
        let entry = ALL_CAPABILITIES
            .iter()
            .find(|m| m.bit == CAP_SECURITY_EVENTS);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().key, "security_events");
        assert!(entry.unwrap().default_enabled);
    }

    #[test]
    fn capability_key_security_events_round_trip() {
        let key: CapabilityKey = "security_events".parse().unwrap();
        assert_eq!(key.to_bit(), CAP_SECURITY_EVENTS);
    }

    #[test]
    fn test_cap_file_bit() {
        assert_eq!(CAP_FILE, 64);
        assert!(has_capability(CAP_FILE, CAP_FILE));
        assert!(!has_capability(CAP_DEFAULT, CAP_FILE));
    }

    #[test]
    fn test_probe_type_to_cap() {
        assert_eq!(probe_type_to_cap("icmp"), Some(CAP_PING_ICMP));
        assert_eq!(probe_type_to_cap("tcp"), Some(CAP_PING_TCP));
        assert_eq!(probe_type_to_cap("http"), Some(CAP_PING_HTTP));
        assert_eq!(probe_type_to_cap("unknown"), None);
    }

    #[test]
    fn test_cap_docker() {
        assert_eq!(CAP_DOCKER, 128);
        assert!(has_capability(CAP_DOCKER, CAP_DOCKER));
        assert!(!has_capability(CAP_DEFAULT, CAP_DOCKER));
    }

    #[test]
    fn test_u32_max_allows_everything() {
        assert!(has_capability(u32::MAX, CAP_TERMINAL));
        assert!(has_capability(u32::MAX, CAP_EXEC));
        assert!(has_capability(u32::MAX, CAP_UPGRADE));
        assert!(has_capability(u32::MAX, CAP_PING_ICMP));
    }

    #[test]
    fn test_capability_key_parse_terminal() {
        assert_eq!(
            "terminal".parse::<CapabilityKey>(),
            Ok(CapabilityKey::Terminal)
        );
    }

    #[test]
    fn test_capability_key_parse_ping_http() {
        assert_eq!(
            "ping_http".parse::<CapabilityKey>(),
            Ok(CapabilityKey::PingHttp)
        );
    }

    #[test]
    fn test_capability_key_parse_unknown_fails() {
        assert!("nope".parse::<CapabilityKey>().is_err());
    }

    #[test]
    fn test_effective_capabilities_masks_server_and_agent_caps() {
        assert_eq!(
            effective_capabilities(CAP_EXEC | CAP_FILE, CAP_FILE),
            CAP_FILE
        );
    }

    #[test]
    fn cap_ip_quality_bit() {
        assert_eq!(CAP_IP_QUALITY, 1024);
        assert_eq!(CAP_VALID_MASK & CAP_IP_QUALITY, CAP_IP_QUALITY);
        assert_eq!(CAP_DEFAULT & CAP_IP_QUALITY, 0); // opt-in, not default
    }

    #[test]
    fn capability_key_ip_quality_round_trip() {
        let key: CapabilityKey = "ip_quality".parse().unwrap();
        assert_eq!(key.to_bit(), CAP_IP_QUALITY);
        assert_eq!(key.as_str(), "ip_quality");
    }

    #[test]
    fn all_capabilities_includes_ip_quality() {
        let entry = ALL_CAPABILITIES.iter().find(|m| m.bit == CAP_IP_QUALITY);
        assert!(entry.is_some());
        assert!(!entry.unwrap().default_enabled);
    }
}
