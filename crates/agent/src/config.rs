use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentConfig {
    pub server_url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub enrollment_code: String,
    #[serde(default)]
    pub collector: CollectorConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub file: FileConfig,
    #[serde(default)]
    pub ip_change: IpChangeConfig,
    #[serde(default)]
    pub upgrade: UpgradeConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub capabilities: CapabilitiesConfig,
}

/// Agent-local capability policy, authored exclusively on the agent host
/// (config file or `SERVERBEE_CAPABILITIES__*` env). The server cannot
/// modify these — it only mirrors what the agent reports.
///
/// `allow`/`deny` are capability keys (e.g. `terminal`, `exec`, `file`,
/// `docker`) applied on top of the built-in default set (`CAP_DEFAULT`).
/// `deny` wins over `allow`. CLI `--allow-cap` / `--deny-cap` flags layer
/// on top of this config for ad-hoc overrides.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CapabilitiesConfig {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    /// Footgun guard: max `--for` the grant CLI accepts. Not a security
    /// boundary (host root can edit the file directly). Default `24h`.
    #[serde(default = "default_temporary_max_duration")]
    pub temporary_max_duration: String,
    /// Directory holding `capability_grants.json`.
    #[serde(default = "default_capability_state_dir")]
    pub state_dir: String,
}

fn default_temporary_max_duration() -> String {
    "24h".to_string()
}

fn default_capability_state_dir() -> String {
    "/var/lib/serverbee".to_string()
}

impl Default for CapabilitiesConfig {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            deny: Vec::new(),
            temporary_max_duration: default_temporary_max_duration(),
            state_dir: default_capability_state_dir(),
        }
    }
}

impl CapabilitiesConfig {
    pub fn grants_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.state_dir).join("capability_grants.json")
    }

    pub fn temporary_max_duration_secs(&self) -> anyhow::Result<i64> {
        crate::capability_grants::parse_duration_secs(&self.temporary_max_duration)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_security_data_dir")]
    pub data_dir: String,
    #[serde(default)]
    pub ssh: SshDetectorConfig,
    #[serde(default)]
    pub port_scan: PortScanConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SshDetectorConfig {
    #[serde(default = "default_ssh_window")]
    pub window_seconds: u32,
    #[serde(default = "default_ssh_threshold")]
    pub failed_threshold: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortScanConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_scan_window")]
    pub window_seconds: u32,
    #[serde(default = "default_scan_threshold")]
    pub distinct_port_threshold: u32,
}

fn default_security_data_dir() -> String {
    "/var/lib/serverbee/security".to_string()
}

fn default_ssh_window() -> u32 {
    60
}

fn default_ssh_threshold() -> u32 {
    10
}

fn default_scan_window() -> u32 {
    30
}

fn default_scan_threshold() -> u32 {
    20
}

impl Default for SshDetectorConfig {
    fn default() -> Self {
        Self {
            window_seconds: default_ssh_window(),
            failed_threshold: default_ssh_threshold(),
        }
    }
}

impl Default for PortScanConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            window_seconds: default_scan_window(),
            distinct_port_threshold: default_scan_threshold(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            data_dir: default_security_data_dir(),
            ssh: SshDetectorConfig::default(),
            port_scan: PortScanConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectorConfig {
    #[serde(default = "default_interval")]
    pub interval: u32,
    #[serde(default)]
    pub enable_gpu: bool,
    #[serde(default = "default_true")]
    pub enable_temperature: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub file: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub root_paths: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    #[serde(default = "default_deny_patterns")]
    pub deny_patterns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpgradeConfig {
    #[serde(default = "default_release_repo")]
    pub release_repo_url: String,
    #[serde(default)]
    pub release_cert_spki_sha256: String,
}

fn default_release_repo() -> String {
    option_env!("SERVERBEE_RELEASE_REPO")
        .unwrap_or("https://github.com/ZingerLittleBee/ServerBee/releases")
        .to_string()
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            release_repo_url: default_release_repo(),
            release_cert_spki_sha256: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IpChangeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_external_ip_urls")]
    pub external_ip_urls: Vec<String>,
    #[serde(default = "default_ip_interval")]
    pub interval_secs: u64,
}

impl Default for IpChangeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            external_ip_urls: default_external_ip_urls(),
            interval_secs: default_ip_interval(),
        }
    }
}

fn default_external_ip_urls() -> Vec<String> {
    // Try multiple public IP services in order — the first to respond wins.
    // Spread across independent operators so any single outage is recoverable.
    vec![
        "https://api.ipify.org".to_string(),
        "https://ifconfig.me/ip".to_string(),
        "https://icanhazip.com".to_string(),
        "https://checkip.amazonaws.com".to_string(),
    ]
}

fn default_ip_interval() -> u64 {
    300
}

fn default_max_file_size() -> u64 {
    1_073_741_824 // 1GB
}

fn default_deny_patterns() -> Vec<String> {
    vec![
        "*.key".into(),
        "*.pem".into(),
        "id_rsa*".into(),
        ".env*".into(),
        "shadow".into(),
        "passwd".into(),
    ]
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            root_paths: Vec::new(),
            max_file_size: default_max_file_size(),
            deny_patterns: default_deny_patterns(),
        }
    }
}

fn default_interval() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            interval: 3,
            enable_gpu: false,
            enable_temperature: true,
        }
    }
}

impl AgentConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config: Self = Figment::new()
            .merge(Toml::file("/etc/serverbee/agent.toml"))
            .merge(Toml::file("agent.toml"))
            .merge(Env::prefixed("SERVERBEE_").split("__"))
            .extract()?;
        Ok(config)
    }

    pub fn config_path_for_persistence() -> &'static str {
        Self::select_config_path_for_persistence(
            std::path::Path::new("agent.toml").exists(),
            std::path::Path::new("/etc/serverbee/agent.toml").exists(),
        )
    }

    pub(crate) fn select_config_path_for_persistence(
        local_exists: bool,
        system_exists: bool,
    ) -> &'static str {
        if local_exists {
            "agent.toml"
        } else if system_exists {
            "/etc/serverbee/agent.toml"
        } else {
            "agent.toml"
        }
    }

    pub(crate) fn token_env_override_present() -> bool {
        std::env::var_os("SERVERBEE_TOKEN").is_some()
    }
}

#[cfg(test)]
pub(crate) fn with_serverbee_token_env<T>(value: Option<&str>, test: impl FnOnce() -> T) -> T {
    use std::sync::{Mutex, OnceLock};

    struct ServerbeeTokenEnvGuard {
        original: Option<std::ffi::OsString>,
    }

    impl Drop for ServerbeeTokenEnvGuard {
        fn drop(&mut self) {
            match self.original.take() {
                Some(value) => unsafe {
                    std::env::set_var("SERVERBEE_TOKEN", value);
                },
                None => unsafe {
                    std::env::remove_var("SERVERBEE_TOKEN");
                },
            }
        }
    }

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _lock = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock");
    let original = std::env::var_os("SERVERBEE_TOKEN");

    match value {
        Some(value) => unsafe {
            std::env::set_var("SERVERBEE_TOKEN", value);
        },
        None => unsafe {
            std::env::remove_var("SERVERBEE_TOKEN");
        },
    }

    let _guard = ServerbeeTokenEnvGuard { original };
    test()
}

#[cfg(test)]
pub(crate) fn assert_config_path() {
    assert_eq!(
        AgentConfig::select_config_path_for_persistence(true, true),
        "agent.toml"
    );
    assert_eq!(
        AgentConfig::select_config_path_for_persistence(false, true),
        "/etc/serverbee/agent.toml"
    );
    assert_eq!(
        AgentConfig::select_config_path_for_persistence(false, false),
        "agent.toml"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_change_config_defaults() {
        let config = IpChangeConfig::default();
        assert!(
            config.enabled,
            "IP change detection should be enabled by default"
        );
        assert_eq!(
            config.interval_secs, 300,
            "default interval should be 300 seconds"
        );
        assert!(
            config.external_ip_urls.len() >= 2,
            "should default to multiple external IP services for redundancy"
        );
        assert_eq!(
            config.external_ip_urls.first().map(String::as_str),
            Some("https://api.ipify.org"),
            "ipify is the primary service"
        );
        // Independent operators so a single-provider outage is recoverable
        assert!(
            config
                .external_ip_urls
                .iter()
                .any(|u| u.contains("ifconfig.me"))
        );
        assert!(
            config
                .external_ip_urls
                .iter()
                .any(|u| u.contains("icanhazip"))
        );
    }

    #[test]
    fn token_env_override_present_detects_serverbee_token() {
        super::with_serverbee_token_env(Some("env-token"), || {
            assert!(AgentConfig::token_env_override_present());
        });
    }

    #[test]
    fn upgrade_config_defaults_to_official_releases() {
        let c = UpgradeConfig::default();
        assert_eq!(
            c.release_repo_url,
            "https://github.com/ZingerLittleBee/ServerBee/releases"
        );
        assert!(c.release_cert_spki_sha256.is_empty());
    }

    #[test]
    fn agent_config_has_upgrade_section_by_default() {
        let c: AgentConfig = figment::Figment::new()
            .merge(figment::providers::Toml::string(
                r#"server_url = "ws://localhost:9527""#,
            ))
            .extract()
            .expect("minimal AgentConfig");
        assert!(c.upgrade.release_repo_url.starts_with("https://"));
    }

    #[test]
    fn security_config_defaults_are_sensible() {
        let s = SecurityConfig::default();
        assert!(s.enabled);
        assert_eq!(s.data_dir, "/var/lib/serverbee/security");
        assert_eq!(s.ssh.window_seconds, 60);
        assert_eq!(s.ssh.failed_threshold, 10);
        assert!(!s.port_scan.enabled);
        assert_eq!(s.port_scan.window_seconds, 30);
        assert_eq!(s.port_scan.distinct_port_threshold, 20);
    }

    #[test]
    fn agent_config_includes_security_defaults() {
        let c: AgentConfig = figment::Figment::new()
            .merge(figment::providers::Toml::string(
                r#"server_url = "ws://localhost:9527""#,
            ))
            .extract()
            .expect("minimal AgentConfig");
        assert!(c.security.enabled);
        assert_eq!(c.security.ssh.failed_threshold, 10);
        assert!(!c.security.port_scan.enabled);
    }

    #[test]
    fn defaults_resolve_grants_path_and_max_duration() {
        let c = CapabilitiesConfig::default();
        assert_eq!(
            c.grants_path(),
            std::path::Path::new("/var/lib/serverbee/capability_grants.json")
        );
        assert_eq!(c.temporary_max_duration_secs().unwrap(), 86_400);
    }

    #[test]
    fn security_config_overrides_from_toml() {
        let c: AgentConfig = figment::Figment::new()
            .merge(figment::providers::Toml::string(
                r#"
server_url = "ws://localhost:9527"
[security]
enabled = false
data_dir = "/tmp/sb"
[security.ssh]
window_seconds = 30
failed_threshold = 5
[security.port_scan]
enabled = true
distinct_port_threshold = 50
"#,
            ))
            .extract()
            .expect("AgentConfig with security overrides");
        assert!(!c.security.enabled);
        assert_eq!(c.security.data_dir, "/tmp/sb");
        assert_eq!(c.security.ssh.window_seconds, 30);
        assert_eq!(c.security.ssh.failed_threshold, 5);
        assert!(c.security.port_scan.enabled);
        assert_eq!(c.security.port_scan.distinct_port_threshold, 50);
    }

    #[test]
    fn capabilities_config_overrides_from_toml() {
        let c: AgentConfig = figment::Figment::new()
            .merge(figment::providers::Toml::string(
                r#"
server_url = "ws://localhost:9527"
[capabilities]
temporary_max_duration = "2h"
state_dir = "/tmp/grants"
"#,
            ))
            .extract()
            .expect("AgentConfig with capabilities overrides");
        assert_eq!(c.capabilities.temporary_max_duration, "2h");
        assert_eq!(c.capabilities.state_dir, "/tmp/grants");
        assert_eq!(c.capabilities.temporary_max_duration_secs().unwrap(), 7_200);
    }
}
