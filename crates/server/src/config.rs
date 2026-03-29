use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use ipnet::IpNet;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AppConfig {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub oauth: OAuthConfig,
    #[serde(default)]
    pub geoip: GeoIpConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub upgrade: UpgradeConfig,
    #[serde(default)]
    pub file: FileConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: default_server(),
            database: DatabaseConfig::default(),
            auth: AuthConfig::default(),
            admin: AdminConfig::default(),
            retention: RetentionConfig::default(),
            rate_limit: RateLimitConfig::default(),
            oauth: OAuthConfig::default(),
            geoip: GeoIpConfig::default(),
            log: LogConfig::default(),
            scheduler: SchedulerConfig::default(),
            upgrade: UpgradeConfig::default(),
            file: FileConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_trusted_proxies")]
    pub trusted_proxies: Vec<IpNet>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
            max_connections: default_max_connections(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_session_ttl")]
    pub session_ttl: i64,
    #[serde(default)]
    pub auto_discovery_key: String,
    /// Whether to set the Secure flag on session cookies.
    /// Defaults to true. Set to false only for development without HTTPS.
    #[serde(default = "default_true")]
    pub secure_cookie: bool,
    /// Maximum number of servers allowed (0 = no limit, best-effort soft cap).
    #[serde(default)]
    pub max_servers: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_ttl: default_session_ttl(),
            auto_discovery_key: String::new(),
            secure_cookie: true,
            max_servers: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    #[serde(default = "default_admin_username")]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            username: default_admin_username(),
            password: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_7")]
    pub records_days: u32,
    #[serde(default = "default_90")]
    pub records_hourly_days: u32,
    #[serde(default = "default_7")]
    pub gpu_records_days: u32,
    #[serde(default = "default_7")]
    pub ping_records_days: u32,
    #[serde(default = "default_180")]
    pub audit_logs_days: u32,
    #[serde(default = "default_7")]
    pub network_probe_days: u32,
    #[serde(default = "default_90")]
    pub network_probe_hourly_days: u32,
    #[serde(default = "default_7")]
    pub traffic_hourly_days: u32,
    #[serde(default = "default_400")]
    pub traffic_daily_days: u32,
    #[serde(default = "default_7")]
    pub task_results_days: u32,
    #[serde(default = "default_7")]
    pub docker_events_days: u32,
    #[serde(default = "default_30")]
    pub service_monitor_days: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            records_days: 7,
            records_hourly_days: 90,
            gpu_records_days: 7,
            ping_records_days: 7,
            audit_logs_days: 180,
            network_probe_days: 7,
            network_probe_hourly_days: 90,
            traffic_hourly_days: 7,
            traffic_daily_days: 400,
            task_results_days: 7,
            docker_events_days: 7,
            service_monitor_days: 30,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RateLimitConfig {
    #[serde(default = "default_5")]
    pub login_max: u32,
    #[serde(default = "default_3")]
    pub register_max: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            login_max: default_5(),
            register_max: default_3(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OAuthConfig {
    #[serde(default)]
    pub github: Option<OAuthProviderConfig>,
    #[serde(default)]
    pub google: Option<OAuthProviderConfig>,
    #[serde(default)]
    pub oidc: Option<OIDCProviderConfig>,
    /// Base URL of the ServerBee server (e.g. "https://serverbee.example.com").
    /// Used to construct OAuth callback URLs.
    #[serde(default)]
    pub base_url: String,
    /// Whether to allow automatic user creation on first OAuth login.
    /// Defaults to false. When false, OAuth login only works for existing linked accounts.
    #[serde(default)]
    pub allow_registration: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OIDCProviderConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "default_oidc_scopes")]
    pub scopes: Vec<String>,
}

fn default_oidc_scopes() -> Vec<String> {
    vec!["openid".to_string(), "email".to_string(), "profile".to_string()]
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct GeoIpConfig {
    #[serde(default)]
    pub mmdb_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub file: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_utc")]
    pub timezone: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            timezone: default_utc(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpgradeConfig {
    #[serde(default = "default_release_base_url")]
    pub release_base_url: String,
}

fn default_release_base_url() -> String {
    "https://github.com/ZingerLittleBee/ServerBee/releases".to_string()
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            release_base_url: default_release_base_url(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    /// Maximum file upload size in bytes. Default: 100 MB.
    #[serde(default = "default_max_upload_size")]
    pub max_upload_size: u64,
}

fn default_max_upload_size() -> u64 {
    104_857_600 // 100 MB
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            max_upload_size: default_max_upload_size(),
        }
    }
}

fn default_utc() -> String {
    "UTC".to_string()
}

// Default functions
fn default_trusted_proxies() -> Vec<IpNet> {
    // Trust private/loopback IPs by default so XFF works out-of-the-box
    // behind Docker, Nginx, Caddy, Kubernetes, etc.
    [
        "127.0.0.0/8",
        "::1/128",
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
        "fc00::/7",
        "fe80::/10",
    ]
    .iter()
    .filter_map(|s| s.parse().ok())
    .collect()
}

fn default_server() -> ServerConfig {
    ServerConfig {
        listen: default_listen(),
        data_dir: default_data_dir(),
        trusted_proxies: default_trusted_proxies(),
    }
}

fn default_listen() -> String {
    "0.0.0.0:9527".to_string()
}

fn default_data_dir() -> String {
    "./data".to_string()
}

fn default_db_path() -> String {
    "serverbee.db".to_string()
}

fn default_max_connections() -> u32 {
    10
}

fn default_session_ttl() -> i64 {
    86400
}

fn default_admin_username() -> String {
    "admin".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_7() -> u32 {
    7
}

fn default_90() -> u32 {
    90
}

fn default_180() -> u32 {
    180
}

fn default_true() -> bool {
    true
}

fn default_5() -> u32 {
    5
}

fn default_3() -> u32 {
    3
}

fn default_30() -> u32 {
    30
}

fn default_400() -> u32 {
    400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timezone_parsing() {
        use chrono_tz::Tz;
        assert!("UTC".parse::<Tz>().is_ok());
        assert!("Asia/Shanghai".parse::<Tz>().is_ok());
        assert!("Invalid/Zone".parse::<Tz>().is_err());
    }

    #[test]
    fn test_default_trusted_proxies_includes_private_ranges() {
        let proxies = default_trusted_proxies();
        assert_eq!(proxies.len(), 7);
        // Verify Docker bridge network is covered
        let docker_ip: std::net::IpAddr = "172.17.0.1".parse().unwrap();
        assert!(proxies.iter().any(|net| net.contains(&docker_ip)));
        // Verify localhost is covered
        let localhost: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        assert!(proxies.iter().any(|net| net.contains(&localhost)));
        // Verify public IP is NOT covered
        let public_ip: std::net::IpAddr = "8.8.8.8".parse().unwrap();
        assert!(!proxies.iter().any(|net| net.contains(&public_ip)));
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config: AppConfig = Figment::new()
            .merge(Toml::file("/etc/serverbee/server.toml"))
            .merge(Toml::file("server.toml"))
            .merge(Env::prefixed("SERVERBEE_").split("__"))
            .extract()?;
        Ok(config)
    }
}
