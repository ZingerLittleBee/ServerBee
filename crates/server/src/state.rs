use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use serverbee_common::protocol::BrowserMessage;
use tokio::sync::broadcast;

use crate::config::AppConfig;
use crate::error::AppError;
use crate::service::agent_manager::AgentManager;
use crate::service::alert::AlertStateManager;
use crate::service::docker_viewer::DockerViewerTracker;
use crate::service::file_transfer::FileTransferManager;
use crate::service::geoip::GeoIpService;
use crate::service::task_scheduler::TaskScheduler;

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
    pub geoip: Arc<std::sync::RwLock<Option<GeoIpService>>>,
    pub geoip_downloading: AtomicBool,
    /// CSRF state tokens for OAuth flow, keyed by state string → provider.
    pub oauth_states: DashMap<String, (String, chrono::DateTime<chrono::Utc>)>,
    /// Pending TOTP secrets for 2FA setup, keyed by user_id.
    pub pending_totp: DashMap<String, PendingTotp>,
    /// Rate limiter for login attempts, keyed by IP.
    pub login_rate_limit: DashMap<String, RateLimitEntry>,
    /// Rate limiter for agent registration attempts, keyed by IP.
    pub register_rate_limit: DashMap<String, RateLimitEntry>,
    /// Manages file download/upload transfers between browser and agent.
    pub file_transfers: Arc<FileTransferManager>,
    /// Tracks browser connections subscribed to Docker updates per server.
    pub docker_viewers: DockerViewerTracker,
    /// Cron-based scheduled task scheduler.
    pub task_scheduler: Arc<TaskScheduler>,
    /// Shared alert state manager for dedup across poll-based and event-driven evaluation.
    pub alert_state_manager: AlertStateManager,
}

static RATE_CHECK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Remove entries older than `window_minutes` from the DashMap.
fn cleanup_expired_entries(map: &DashMap<String, RateLimitEntry>, window_minutes: i64) {
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(window_minutes);
    map.retain(|_, entry| entry.window_start > cutoff);
}

impl AppState {
    /// Check rate limit against a given DashMap. Returns true if allowed.
    fn check_rate(map: &DashMap<String, RateLimitEntry>, ip: &str, max: u32) -> bool {
        let count = RATE_CHECK_COUNTER.fetch_add(1, Ordering::Relaxed);
        if count % 100 == 0 {
            cleanup_expired_entries(map, 15);
        }

        let now = chrono::Utc::now();
        let window = chrono::Duration::minutes(15);

        let mut entry = map.entry(ip.to_string()).or_insert_with(|| RateLimitEntry {
            count: 0,
            window_start: now,
        });

        // Reset window if expired
        if now - entry.window_start > window {
            entry.count = 1;
            entry.window_start = now;
            return true;
        }

        // Check before incrementing so denied requests don't grow the counter
        if entry.count >= max {
            return false;
        }

        entry.count += 1;
        true
    }

    /// Check if an IP has exceeded the login rate limit.
    /// Returns true if allowed, false if rate-limited.
    pub fn check_login_rate(&self, ip: &str) -> bool {
        Self::check_rate(
            &self.login_rate_limit,
            ip,
            self.config.rate_limit.login_max,
        )
    }

    /// Check if an IP has exceeded the registration rate limit.
    /// Returns true if allowed, false if rate-limited.
    pub fn check_register_rate(&self, ip: &str) -> bool {
        Self::check_rate(
            &self.register_rate_limit,
            ip,
            self.config.rate_limit.register_max,
        )
    }

    pub async fn new(db: DatabaseConnection, config: AppConfig) -> Result<Arc<Self>, AppError> {
        let (browser_tx, _) = broadcast::channel(256);
        let agent_manager = AgentManager::new(browser_tx.clone());
        let geoip = if !config.geoip.mmdb_path.is_empty() {
            GeoIpService::load(&config.geoip.mmdb_path)
        } else {
            let default_path = std::path::Path::new(&config.server.data_dir)
                .join(crate::service::geoip::DBIP_FILENAME);
            GeoIpService::load(&default_path.display().to_string())
        };
        if geoip.is_some() {
            tracing::info!("GeoIP database loaded");
        } else {
            tracing::info!("GeoIP database not available — download via Settings or Server Map widget");
        }
        let file_transfers = Arc::new(FileTransferManager::new(
            std::env::temp_dir().join("serverbee-transfers"),
        ));
        let task_scheduler = Arc::new(TaskScheduler::new(&config.scheduler.timezone).await?);
        let alert_state_manager = match AlertStateManager::load_from_db(&db).await {
            Ok(sm) => sm,
            Err(e) => {
                tracing::warn!("Failed to load alert states from DB, starting empty: {e}");
                AlertStateManager::new()
            }
        };
        // Preload capabilities and features from DB
        if let Err(e) = agent_manager.preload_capabilities(&db).await {
            tracing::warn!("Failed to preload capabilities: {e}");
        }
        Ok(Arc::new(Self {
            db,
            agent_manager,
            browser_tx,
            config,
            geoip: Arc::new(std::sync::RwLock::new(geoip)),
            geoip_downloading: AtomicBool::new(false),
            oauth_states: DashMap::new(),
            pending_totp: DashMap::new(),
            login_rate_limit: DashMap::new(),
            register_rate_limit: DashMap::new(),
            file_transfers,
            docker_viewers: DockerViewerTracker::new(),
            task_scheduler,
            alert_state_manager,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_entries_are_cleaned() {
        let map: DashMap<String, RateLimitEntry> = DashMap::new();
        map.insert(
            "old_ip".to_string(),
            RateLimitEntry {
                count: 5,
                window_start: chrono::Utc::now() - chrono::Duration::minutes(20),
            },
        );
        map.insert(
            "new_ip".to_string(),
            RateLimitEntry {
                count: 1,
                window_start: chrono::Utc::now(),
            },
        );

        cleanup_expired_entries(&map, 15);
        assert!(!map.contains_key("old_ip"));
        assert!(map.contains_key("new_ip"));
    }
}
