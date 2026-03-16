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

    pub fn config_path() -> &'static str {
        if std::path::Path::new("/etc/serverbee/agent.toml").exists() {
            "/etc/serverbee/agent.toml"
        } else {
            "agent.toml"
        }
    }
}
