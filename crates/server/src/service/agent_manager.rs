use std::net::SocketAddr;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, oneshot};

use serverbee_common::constants::{CAP_DOCKER, has_capability};
use serverbee_common::docker_types::*;
use serverbee_common::protocol::{AgentMessage, BrowserMessage, ServerMessage};
use serverbee_common::types::{ServerStatus, SystemReport};

/// Sender for forwarding terminal output from agent to browser WS.
pub type TerminalOutputTx = mpsc::Sender<TerminalSessionEvent>;

/// Events sent from agent handler to browser terminal WS.
pub enum TerminalSessionEvent {
    Output(String), // base64 encoded data
    Started,
    Error(String),
}

pub struct AgentManager {
    connections: DashMap<String, AgentConnection>,
    latest_reports: DashMap<String, CachedReport>,
    browser_tx: broadcast::Sender<BrowserMessage>,
    /// Maps session_id -> terminal output channel (for routing agent output to browser WS)
    terminal_sessions: DashMap<String, TerminalOutputTx>,
    /// Maps msg_id -> (oneshot sender, creation time) for HTTP→WS relay
    pending_requests: DashMap<String, (oneshot::Sender<AgentMessage>, std::time::Instant)>,
    // Docker caches
    docker_containers: DashMap<String, Vec<DockerContainer>>,
    docker_stats: DashMap<String, Vec<DockerContainerStats>>,
    docker_info: DashMap<String, DockerSystemInfo>,
    features: DashMap<String, Vec<String>>,
    capabilities: DashMap<String, u32>,
    /// Maps server_id -> (session_id -> log entry sender)
    docker_log_sessions: DashMap<String, DashMap<String, mpsc::Sender<Vec<DockerLogEntry>>>>,
}

#[allow(dead_code)]
pub struct AgentConnection {
    pub server_id: String,
    pub server_name: String,
    pub tx: mpsc::Sender<ServerMessage>,
    pub connected_at: Instant,
    pub last_report_at: Instant,
    pub remote_addr: SocketAddr,
    pub protocol_version: u32,
}

#[allow(dead_code)]
pub struct CachedReport {
    pub report: SystemReport,
    pub received_at: Instant,
}

impl AgentManager {
    pub fn new(browser_tx: broadcast::Sender<BrowserMessage>) -> Self {
        Self {
            connections: DashMap::new(),
            latest_reports: DashMap::new(),
            browser_tx,
            terminal_sessions: DashMap::new(),
            pending_requests: DashMap::new(),
            docker_containers: DashMap::new(),
            docker_stats: DashMap::new(),
            docker_info: DashMap::new(),
            features: DashMap::new(),
            capabilities: DashMap::new(),
            docker_log_sessions: DashMap::new(),
        }
    }

    /// Register a new agent connection and broadcast ServerOnline to browsers.
    pub fn add_connection(
        &self,
        server_id: String,
        server_name: String,
        tx: mpsc::Sender<ServerMessage>,
        remote_addr: SocketAddr,
    ) {
        let now = Instant::now();
        self.connections.insert(
            server_id.clone(),
            AgentConnection {
                server_id: server_id.clone(),
                server_name,
                tx,
                connected_at: now,
                last_report_at: now,
                remote_addr,
                protocol_version: 1,
            },
        );

        let _ = self
            .browser_tx
            .send(BrowserMessage::ServerOnline { server_id });
    }

    /// Unregister an agent connection and broadcast ServerOffline to browsers.
    pub fn remove_connection(&self, server_id: &str) {
        self.connections.remove(server_id);
        self.remove_docker_log_sessions_for_server(server_id);

        let _ = self.browser_tx.send(BrowserMessage::ServerOffline {
            server_id: server_id.to_string(),
        });
    }

    /// Update the latest report for a server and broadcast an Update to browsers.
    pub fn update_report(&self, server_id: &str, report: SystemReport) {
        let now = Instant::now();

        // Update last_report_at on the connection
        if let Some(mut conn) = self.connections.get_mut(server_id) {
            conn.last_report_at = now;
        }

        // Build a ServerStatus for the broadcast. Static fields (mem_total, disk_total,
        // os, cpu_name, etc.) are not available here -- set them to defaults since the
        // browser can merge with REST data.
        let status = ServerStatus {
            id: server_id.to_string(),
            name: self
                .connections
                .get(server_id)
                .map(|c| c.server_name.clone())
                .unwrap_or_default(),
            online: true,
            last_active: chrono::Utc::now().timestamp(),
            uptime: report.uptime,
            cpu: report.cpu,
            mem_used: report.mem_used,
            mem_total: 0,
            swap_used: report.swap_used,
            swap_total: 0,
            disk_used: report.disk_used,
            disk_total: 0,
            net_in_speed: report.net_in_speed,
            net_out_speed: report.net_out_speed,
            net_in_transfer: report.net_in_transfer,
            net_out_transfer: report.net_out_transfer,
            load1: report.load1,
            load5: report.load5,
            load15: report.load15,
            tcp_conn: report.tcp_conn,
            udp_conn: report.udp_conn,
            process_count: report.process_count,
            cpu_name: None,
            os: None,
            region: None,
            country_code: None,
            group_id: None,
            features: vec![],
        };

        let _ = self.browser_tx.send(BrowserMessage::Update {
            servers: vec![status],
        });

        // Cache the report
        self.latest_reports.insert(
            server_id.to_string(),
            CachedReport {
                report,
                received_at: now,
            },
        );
    }

    /// Check if a server is currently connected.
    pub fn is_online(&self, server_id: &str) -> bool {
        self.connections.contains_key(server_id)
    }

    /// Return the number of currently connected agents.
    #[allow(dead_code)]
    pub fn online_count(&self) -> usize {
        self.connections.len()
    }

    /// Get the latest cached report for a server.
    pub fn get_latest_report(&self, server_id: &str) -> Option<SystemReport> {
        self.latest_reports.get(server_id).map(|r| r.report.clone())
    }

    /// Get all latest cached reports as (server_id, report) pairs.
    pub fn all_latest_reports(&self) -> Vec<(String, SystemReport)> {
        self.latest_reports
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().report.clone()))
            .collect()
    }

    /// Get the mpsc sender for a specific agent to send commands to it.
    pub fn get_sender(&self, server_id: &str) -> Option<mpsc::Sender<ServerMessage>> {
        self.connections.get(server_id).map(|c| c.tx.clone())
    }

    /// Get all connected agent server IDs.
    pub fn connected_server_ids(&self) -> Vec<String> {
        self.connections.iter().map(|e| e.key().clone()).collect()
    }

    /// Get the remote address of a connected agent.
    pub fn get_remote_addr(&self, server_id: &str) -> Option<SocketAddr> {
        self.connections.get(server_id).map(|c| c.remote_addr)
    }

    /// Update the last_report_at timestamp for a connection (e.g., on Pong).
    pub fn touch_connection(&self, server_id: &str) {
        if let Some(mut conn) = self.connections.get_mut(server_id) {
            conn.last_report_at = Instant::now();
        }
    }

    /// Register a terminal session for routing output from agent to browser.
    pub fn register_terminal_session(&self, session_id: String, tx: TerminalOutputTx) {
        self.terminal_sessions.insert(session_id, tx);
    }

    /// Unregister a terminal session.
    pub fn unregister_terminal_session(&self, session_id: &str) {
        self.terminal_sessions.remove(session_id);
    }

    /// Get the terminal output sender for a session.
    pub fn get_terminal_session(&self, session_id: &str) -> Option<TerminalOutputTx> {
        self.terminal_sessions.get(session_id).map(|v| v.clone())
    }

    /// Find agents that have not reported for `threshold_secs` seconds,
    /// remove them, and return their server IDs.
    pub fn check_offline(&self, threshold_secs: u64) -> Vec<String> {
        let now = Instant::now();
        let mut offline_ids = Vec::new();

        // Collect IDs of agents that are past the threshold
        for entry in self.connections.iter() {
            let elapsed = now.duration_since(entry.value().last_report_at);
            if elapsed.as_secs() >= threshold_secs {
                offline_ids.push(entry.key().clone());
            }
        }

        // Remove each offline agent (this also broadcasts ServerOffline)
        for id in &offline_ids {
            self.remove_connection(id);
        }

        offline_ids
    }

    pub fn set_protocol_version(&self, server_id: &str, version: u32) {
        if let Some(mut conn) = self.connections.get_mut(server_id) {
            conn.protocol_version = version;
        }
    }

    pub fn get_protocol_version(&self, server_id: &str) -> Option<u32> {
        self.connections.get(server_id).map(|c| c.protocol_version)
    }

    pub fn broadcast_browser(&self, msg: BrowserMessage) {
        let _ = self.browser_tx.send(msg);
    }

    /// Register a pending request for HTTP→WS relay.
    /// Returns a oneshot receiver that will receive the agent's response.
    pub fn register_pending_request(&self, msg_id: String) -> oneshot::Receiver<AgentMessage> {
        let (tx, rx) = oneshot::channel();
        self.pending_requests
            .insert(msg_id, (tx, std::time::Instant::now()));
        rx
    }

    /// Dispatch a response from the agent to a pending HTTP request.
    /// Returns true if the response was delivered, false if no pending request was found.
    pub fn dispatch_pending_response(&self, msg_id: &str, message: AgentMessage) -> bool {
        if let Some((_, (tx, _))) = self.pending_requests.remove(msg_id) {
            let _ = tx.send(message);
            true
        } else {
            false
        }
    }

    // --- Docker cache methods ---

    pub fn update_docker_containers(&self, server_id: &str, containers: Vec<DockerContainer>) {
        self.docker_containers
            .insert(server_id.to_string(), containers);
    }

    pub fn get_docker_containers(&self, server_id: &str) -> Option<Vec<DockerContainer>> {
        self.docker_containers.get(server_id).map(|v| v.clone())
    }

    pub fn update_docker_stats(&self, server_id: &str, stats: Vec<DockerContainerStats>) {
        self.docker_stats.insert(server_id.to_string(), stats);
    }

    pub fn get_docker_stats(&self, server_id: &str) -> Option<Vec<DockerContainerStats>> {
        self.docker_stats.get(server_id).map(|v| v.clone())
    }

    pub fn update_docker_info(&self, server_id: &str, info: DockerSystemInfo) {
        self.docker_info.insert(server_id.to_string(), info);
    }

    pub fn get_docker_info(&self, server_id: &str) -> Option<DockerSystemInfo> {
        self.docker_info.get(server_id).map(|v| v.clone())
    }

    pub fn clear_docker_caches(&self, server_id: &str) {
        self.docker_containers.remove(server_id);
        self.docker_stats.remove(server_id);
        self.docker_info.remove(server_id);
    }

    // --- Features cache ---

    pub fn update_features(&self, server_id: &str, features: Vec<String>) {
        self.features.insert(server_id.to_string(), features);
    }

    pub fn has_feature(&self, server_id: &str, feature: &str) -> bool {
        self.features
            .get(server_id)
            .is_some_and(|f| f.contains(&feature.to_string()))
    }

    // --- Capabilities cache ---

    pub fn update_capabilities(&self, server_id: &str, caps: u32) {
        self.capabilities.insert(server_id.to_string(), caps);
    }

    pub fn has_docker_capability(&self, server_id: &str) -> bool {
        self.capabilities
            .get(server_id)
            .is_some_and(|cap| has_capability(*cap, CAP_DOCKER))
    }

    pub async fn preload_capabilities(
        &self,
        db: &sea_orm::DatabaseConnection,
    ) -> Result<(), sea_orm::DbErr> {
        use crate::entity::server;
        use sea_orm::{EntityTrait, QuerySelect};

        let servers = server::Entity::find()
            .select_only()
            .column(server::Column::Id)
            .column(server::Column::Capabilities)
            .column(server::Column::Features)
            .into_tuple::<(String, i32, String)>()
            .all(db)
            .await?;
        for (id, caps, features_json) in servers {
            self.capabilities.insert(id.clone(), caps as u32);
            let features: Vec<String> = serde_json::from_str(&features_json).unwrap_or_default();
            self.features.insert(id, features);
        }
        Ok(())
    }

    // --- Docker log session routing ---

    pub fn add_docker_log_session(
        &self,
        server_id: &str,
        session_id: String,
        tx: mpsc::Sender<Vec<DockerLogEntry>>,
    ) {
        self.docker_log_sessions
            .entry(server_id.to_string())
            .or_default()
            .insert(session_id, tx);
    }

    pub fn get_docker_log_session(
        &self,
        server_id: &str,
        session_id: &str,
    ) -> Option<mpsc::Sender<Vec<DockerLogEntry>>> {
        self.docker_log_sessions
            .get(server_id)?
            .get(session_id)
            .map(|tx| tx.clone())
    }

    pub fn remove_docker_log_session(&self, server_id: &str, session_id: &str) -> bool {
        if let Some(inner) = self.docker_log_sessions.get(server_id) {
            return inner.remove(session_id).is_some();
        }
        false
    }

    pub fn remove_docker_log_sessions_for_server(&self, server_id: &str) -> Vec<String> {
        if let Some((_, inner)) = self.docker_log_sessions.remove(server_id) {
            inner.into_iter().map(|(id, _)| id).collect()
        } else {
            vec![]
        }
    }

    /// Remove pending requests older than `max_age`.
    pub fn cleanup_expired_requests(&self, max_age: std::time::Duration) {
        let now = std::time::Instant::now();
        self.pending_requests
            .retain(|_, (_, created_at)| now.duration_since(*created_at) < max_age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::protocol::AgentMessage;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    fn make_manager() -> (AgentManager, broadcast::Receiver<BrowserMessage>) {
        let (tx, rx) = broadcast::channel(16);
        (AgentManager::new(tx), rx)
    }

    #[test]
    fn test_add_and_remove_connection() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Server1".into(), tx, test_addr());
        assert!(mgr.is_online("s1"));
        assert_eq!(mgr.online_count(), 1);
        mgr.remove_connection("s1");
        assert!(!mgr.is_online("s1"));
        assert_eq!(mgr.online_count(), 0);
    }

    #[test]
    fn test_broadcast_online_offline() {
        let (mgr, mut rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());
        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg, BrowserMessage::ServerOnline { server_id } if server_id == "s1"));
        mgr.remove_connection("s1");
        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg, BrowserMessage::ServerOffline { server_id } if server_id == "s1"));
    }

    #[test]
    fn test_update_report_and_cache() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());
        let report = SystemReport {
            cpu: 42.5,
            mem_used: 8_000_000_000,
            ..Default::default()
        };
        mgr.update_report("s1", report);
        let cached = mgr.get_latest_report("s1").unwrap();
        assert!((cached.cpu - 42.5).abs() < f64::EPSILON);
        assert_eq!(cached.mem_used, 8_000_000_000);
    }

    #[test]
    fn test_all_latest_reports() {
        let (mgr, _rx) = make_manager();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "A".into(), tx1, test_addr());
        mgr.add_connection("s2".into(), "B".into(), tx2, test_addr());
        mgr.update_report(
            "s1",
            SystemReport {
                cpu: 10.0,
                ..Default::default()
            },
        );
        mgr.update_report(
            "s2",
            SystemReport {
                cpu: 20.0,
                ..Default::default()
            },
        );
        let all = mgr.all_latest_reports();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_connected_server_ids() {
        let (mgr, _rx) = make_manager();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "A".into(), tx1, test_addr());
        mgr.add_connection("s2".into(), "B".into(), tx2, test_addr());
        let ids = mgr.connected_server_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"s1".to_string()));
        assert!(ids.contains(&"s2".to_string()));
    }

    #[test]
    fn test_terminal_session_lifecycle() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.register_terminal_session("sess1".into(), tx);
        assert!(mgr.get_terminal_session("sess1").is_some());
        mgr.unregister_terminal_session("sess1");
        assert!(mgr.get_terminal_session("sess1").is_none());
    }

    #[test]
    fn test_check_offline() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Old".into(), tx, test_addr());
        let offline = mgr.check_offline(0);
        assert_eq!(offline, vec!["s1"]);
        assert!(!mgr.is_online("s1"));
    }

    #[test]
    fn test_check_offline_within_threshold() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Fresh".into(), tx, test_addr());
        let offline = mgr.check_offline(9999);
        assert!(offline.is_empty());
        assert!(mgr.is_online("s1"));
    }

    #[test]
    fn test_protocol_version() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());
        assert_eq!(mgr.get_protocol_version("s1"), Some(1));
        mgr.set_protocol_version("s1", 2);
        assert_eq!(mgr.get_protocol_version("s1"), Some(2));
    }

    #[test]
    fn test_get_report_nonexistent() {
        let (mgr, _rx) = make_manager();
        assert!(mgr.get_latest_report("nope").is_none());
    }

    #[test]
    fn test_cleanup_expired_requests() {
        let (mgr, _rx) = make_manager();
        let _rx1 = mgr.register_pending_request("old".into());
        // Cleanup with zero duration removes everything
        mgr.cleanup_expired_requests(std::time::Duration::from_secs(0));
        let dispatched = mgr.dispatch_pending_response(
            "old",
            AgentMessage::FileOpResult {
                msg_id: "old".into(),
                success: true,
                error: None,
            },
        );
        assert!(!dispatched); // should have been cleaned up
    }

    #[test]
    fn test_pending_request_lifecycle() {
        let (mgr, _rx) = make_manager();
        let mut rx = mgr.register_pending_request("req1".into());
        assert!(rx.try_recv().is_err());

        let dispatched = mgr.dispatch_pending_response(
            "req1",
            AgentMessage::FileOpResult {
                msg_id: "req1".into(),
                success: true,
                error: None,
            },
        );
        assert!(dispatched);

        let dispatched2 = mgr.dispatch_pending_response(
            "req1",
            AgentMessage::FileOpResult {
                msg_id: "req1".into(),
                success: true,
                error: None,
            },
        );
        assert!(!dispatched2);
    }
}
