use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serverbee_common::protocol::BrowserMessage;
use tokio::sync::broadcast;

use crate::config::AppConfig;
use crate::service::agent_manager::AgentManager;

pub struct AppState {
    pub db: DatabaseConnection,
    pub agent_manager: AgentManager,
    pub browser_tx: broadcast::Sender<BrowserMessage>,
    pub config: AppConfig,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: AppConfig) -> Arc<Self> {
        let (browser_tx, _) = broadcast::channel(256);
        let agent_manager = AgentManager::new(browser_tx.clone());
        Arc::new(Self {
            db,
            agent_manager,
            browser_tx,
            config,
        })
    }
}
