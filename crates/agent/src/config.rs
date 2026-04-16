use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    pub server_url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub auto_discovery_key: String,
    #[serde(default)]
    pub collector: CollectorConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub file: FileConfig,
    #[serde(default)]
    pub ip_change: IpChangeConfig,
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
pub struct IpChangeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub check_external_ip: bool,
    #[serde(default = "default_external_ip_url")]
    pub external_ip_url: String,
    #[serde(default = "default_ip_interval")]
    pub interval_secs: u64,
}

impl Default for IpChangeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_external_ip: false,
            external_ip_url: default_external_ip_url(),
            interval_secs: default_ip_interval(),
        }
    }
}

fn default_external_ip_url() -> String {
    "https://api.ipify.org".to_string()
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
        assert!(
            !config.check_external_ip,
            "external IP checking should be disabled by default"
        );
        assert_eq!(
            config.interval_secs, 300,
            "default interval should be 300 seconds"
        );
        assert_eq!(
            config.external_ip_url, "https://api.ipify.org",
            "default external IP URL should be api.ipify.org"
        );
    }
}
