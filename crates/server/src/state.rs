use std::sync::Arc;

use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use serverbee_common::protocol::BrowserMessage;
use tokio::sync::broadcast;

use crate::config::AppConfig;
use crate::service::agent_manager::AgentManager;
use crate::service::geoip::GeoIpService;

/// Pending TOTP setup data, keyed by user_id.
pub struct PendingTotp {
    pub secret: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Rate limiter entry: (count, window_start).
pub struct RateLimitEntry {
    pub count: u32,
    pub window_start: chrono::DateTime<chrono::Utc>,
}

pub struct AppState {
    pub db: DatabaseConnection,
    pub agent_manager: AgentManager,
    pub browser_tx: broadcast::Sender<BrowserMessage>,
    pub config: AppConfig,
    pub geoip: Option<GeoIpService>,
    /// CSRF state tokens for OAuth flow, keyed by state string → provider.
    pub oauth_states: DashMap<String, (String, chrono::DateTime<chrono::Utc>)>,
    /// Pending TOTP secrets for 2FA setup, keyed by user_id.
    pub pending_totp: DashMap<String, PendingTotp>,
    /// Rate limiter for login attempts, keyed by IP.
    pub login_rate_limit: DashMap<String, RateLimitEntry>,
}

impl AppState {
    /// Check if an IP has exceeded the login rate limit.
    /// Returns true if allowed, false if rate-limited.
    pub fn check_login_rate(&self, ip: &str) -> bool {
        let now = chrono::Utc::now();
        let window = chrono::Duration::minutes(15);
        let max = self.config.rate_limit.login_max;

        let mut entry = self
            .login_rate_limit
            .entry(ip.to_string())
            .or_insert_with(|| RateLimitEntry {
                count: 0,
                window_start: now,
            });

        // Reset window if expired
        if now - entry.window_start > window {
            entry.count = 1;
            entry.window_start = now;
            return true;
        }

        entry.count += 1;
        entry.count <= max
    }

    pub fn new(db: DatabaseConnection, config: AppConfig) -> Arc<Self> {
        let (browser_tx, _) = broadcast::channel(256);
        let agent_manager = AgentManager::new(browser_tx.clone());
        let geoip = if config.geoip.enabled {
            GeoIpService::load(&config.geoip.mmdb_path)
        } else {
            None
        };
        Arc::new(Self {
            db,
            agent_manager,
            browser_tx,
            config,
            geoip,
            oauth_states: DashMap::new(),
            pending_totp: DashMap::new(),
            login_rate_limit: DashMap::new(),
        })
    }
}
