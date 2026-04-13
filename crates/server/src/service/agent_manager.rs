use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};

use serverbee_common::constants::{CAP_DOCKER, effective_capabilities, has_capability};
use serverbee_common::docker_types::*;
use serverbee_common::protocol::{AgentMessage, BrowserMessage, ServerMessage};
use serverbee_common::types::{ServerStatus, SystemReport, TracerouteHop};

/// Sender for forwarding terminal output from agent to browser WS.
pub type TerminalOutputTx = mpsc::Sender<TerminalSessionEvent>;

/// Events sent from agent handler to browser terminal WS.
pub enum TerminalSessionEvent {
    Output(String), // base64 encoded data
    Started,
    Error(String),
}

pub struct TracerouteResultData {
    pub target: String,
    pub hops: Vec<TracerouteHop>,
    pub completed: bool,
    pub error: Option<String>,
}

pub struct TracerouteResultEntry {
    pub server_id: String,
    pub result: TracerouteResultData,
    pub created_at: Instant,
}

pub struct AgentManager {
    connections: DashMap<String, AgentConnection>,
    latest_reports: DashMap<String, CachedReport>,
    browser_tx: broadcast::Sender<BrowserMessage>,
    /// Maps session_id -> terminal output channel (for routing agent output to browser WS)
    terminal_sessions: DashMap<String, TerminalOutputTx>,
    /// Maps msg_id -> (oneshot sender, creation time, TTL) for HTTP→WS relay
    pending_requests: DashMap<
        String,
        (
            oneshot::Sender<AgentMessage>,
            std::time::Instant,
            std::time::Duration,
        ),
    >,
    // Docker caches
    docker_containers: DashMap<String, Vec<DockerContainer>>,
    docker_stats: DashMap<String, Vec<DockerContainerStats>>,
    docker_info: DashMap<String, DockerSystemInfo>,
    features: DashMap<String, Vec<String>>,
    server_capabilities: DashMap<String, u32>,
    agent_local_capabilities: DashMap<String, u32>,
    /// Maps server_id -> (session_id -> log entry sender)
    docker_log_sessions: DashMap<String, DashMap<String, mpsc::Sender<Vec<DockerLogEntry>>>>,
    /// Maps request_id -> traceroute result entry (cached for polling)
    traceroute_results: DashMap<String, TracerouteResultEntry>,
    server_lifecycle_locks: DashMap<String, Arc<Mutex<()>>>,
    next_connection_id: AtomicU64,
}

#[allow(dead_code)]
pub struct AgentConnection {
    pub connection_id: u64,
    pub server_id: String,
    pub server_name: String,
    pub tx: mpsc::Sender<ServerMessage>,
    pub connected_at: Instant,
    pub last_report_at: Instant,
    pub remote_addr: SocketAddr,
    pub protocol_version: u32,
    pub os: String,
    pub arch: String,
}

#[allow(dead_code)]
pub struct CachedReport {
    pub report: Arc<SystemReport>,
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
            server_capabilities: DashMap::new(),
            agent_local_capabilities: DashMap::new(),
            docker_log_sessions: DashMap::new(),
            traceroute_results: DashMap::new(),
            server_lifecycle_locks: DashMap::new(),
            next_connection_id: AtomicU64::new(1),
        }
    }

    /// Register a new agent connection and broadcast ServerOnline to browsers.
    pub fn add_connection(
        &self,
        server_id: String,
        server_name: String,
        tx: mpsc::Sender<ServerMessage>,
        remote_addr: SocketAddr,
    ) -> u64 {
        let now = Instant::now();
        let connection_id = self.next_connection_id.fetch_add(1, Ordering::Relaxed);
        self.connections.insert(
            server_id.clone(),
            AgentConnection {
                connection_id,
                server_id: server_id.clone(),
                server_name,
                tx,
                connected_at: now,
                last_report_at: now,
                remote_addr,
                protocol_version: 1,
                os: String::new(),
                arch: String::new(),
            },
        );

        let _ = self
            .browser_tx
            .send(BrowserMessage::ServerOnline { server_id });

        connection_id
    }

    /// Unregister an agent connection and broadcast ServerOffline to browsers.
    pub fn remove_connection(&self, server_id: &str) {
        if self.connections.remove(server_id).is_some() {
            self.finish_connection_removal(server_id);
        }
    }

    pub fn remove_connection_if_current(&self, server_id: &str, expected_connection_id: u64) -> bool {
        let removed = self.connections.remove_if(server_id, |_, connection| {
            connection.connection_id == expected_connection_id
        });
        if removed.is_some() {
            self.finish_connection_removal(server_id);
            true
        } else {
            false
        }
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
                report: Arc::new(report),
                received_at: now,
            },
        );
    }

    /// Check if a server is currently connected.
    pub fn is_online(&self, server_id: &str) -> bool {
        self.connections.contains_key(server_id)
    }

    pub fn server_cleanup_lock(&self, server_id: &str) -> Arc<Mutex<()>> {
        self.server_lifecycle_locks
            .entry(server_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Return the number of currently connected agents.
    #[allow(dead_code)]
    pub fn online_count(&self) -> usize {
        self.connections.len()
    }

    /// Get the latest cached report for a server.
    pub fn get_latest_report(&self, server_id: &str) -> Option<Arc<SystemReport>> {
        self.latest_reports
            .get(server_id)
            .map(|r| Arc::clone(&r.report))
    }

    /// Get all latest cached reports as (server_id, report) pairs.
    pub fn all_latest_reports(&self) -> Vec<(String, Arc<SystemReport>)> {
        self.latest_reports
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(&entry.value().report)))
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
        let mut stale_connections = Vec::new();

        // Collect IDs of agents that are past the threshold
        for entry in self.connections.iter() {
            let elapsed = now.duration_since(entry.value().last_report_at);
            if elapsed.as_secs() >= threshold_secs {
                stale_connections.push((entry.key().clone(), entry.value().connection_id));
            }
        }

        let mut offline_ids = Vec::new();
        for (server_id, connection_id) in stale_connections {
            if self.remove_connection_if_current(&server_id, connection_id) {
                offline_ids.push(server_id);
            }
        }

        offline_ids
    }

    fn finish_connection_removal(&self, server_id: &str) {
        self.server_capabilities.remove(server_id);
        self.agent_local_capabilities.remove(server_id);
        self.remove_docker_log_sessions_for_server(server_id);
        self.clear_docker_caches(server_id);

        let _ = self.browser_tx.send(BrowserMessage::ServerOffline {
            server_id: server_id.to_string(),
        });
    }

    pub fn set_protocol_version(&self, server_id: &str, version: u32) {
        if let Some(mut conn) = self.connections.get_mut(server_id) {
            conn.protocol_version = version;
        }
    }

    pub fn get_protocol_version(&self, server_id: &str) -> Option<u32> {
        self.connections.get(server_id).map(|c| c.protocol_version)
    }

    pub fn update_agent_platform(&self, server_id: &str, os: String, arch: String) {
        if let Some(mut conn) = self.connections.get_mut(server_id) {
            conn.os = os;
            conn.arch = arch;
        }
    }

    pub fn get_agent_platform(&self, server_id: &str) -> Option<(String, String)> {
        self.connections
            .get(server_id)
            .map(|c| (c.os.clone(), c.arch.clone()))
    }

    pub fn broadcast_browser(&self, msg: BrowserMessage) {
        let _ = self.browser_tx.send(msg);
    }

    /// Register a pending request for HTTP→WS relay with a custom TTL.
    /// Returns a oneshot receiver that will receive the agent's response.
    pub fn register_pending_request_with_ttl(
        &self,
        msg_id: String,
        ttl: std::time::Duration,
    ) -> oneshot::Receiver<AgentMessage> {
        let (tx, rx) = oneshot::channel();
        self.pending_requests
            .insert(msg_id, (tx, std::time::Instant::now(), ttl));
        rx
    }

    /// Check if a pending request exists for the given msg_id.
    pub fn has_pending_request(&self, msg_id: &str) -> bool {
        self.pending_requests.contains_key(msg_id)
    }

    /// Register a pending request for HTTP→WS relay with a default 60s TTL.
    /// Returns a oneshot receiver that will receive the agent's response.
    pub fn register_pending_request(&self, msg_id: String) -> oneshot::Receiver<AgentMessage> {
        self.register_pending_request_with_ttl(msg_id, std::time::Duration::from_secs(60))
    }

    /// Dispatch a response from the agent to a pending HTTP request.
    /// Returns true if the response was delivered, false if no pending request was found.
    pub fn dispatch_pending_response(&self, msg_id: &str, message: AgentMessage) -> bool {
        if let Some((_, (tx, _, _))) = self.pending_requests.remove(msg_id) {
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

    pub fn get_features(&self, server_id: &str) -> Vec<String> {
        self.features
            .get(server_id)
            .map(|features| features.clone())
            .unwrap_or_default()
    }

    pub fn has_feature(&self, server_id: &str, feature: &str) -> bool {
        self.features
            .get(server_id)
            .is_some_and(|f| f.contains(&feature.to_string()))
    }

    // --- Capabilities cache ---

    pub fn update_server_capabilities(&self, server_id: &str, caps: u32) {
        self.server_capabilities.insert(server_id.to_string(), caps);
    }

    pub fn update_capabilities(&self, server_id: &str, caps: u32) {
        self.update_server_capabilities(server_id, caps);
    }

    pub fn get_server_capabilities(&self, server_id: &str) -> Option<u32> {
        self.server_capabilities.get(server_id).map(|cap| *cap)
    }

    pub fn update_agent_local_capabilities(&self, server_id: &str, caps: u32) {
        self.agent_local_capabilities
            .insert(server_id.to_string(), caps);
    }

    pub fn get_agent_local_capabilities(&self, server_id: &str) -> Option<u32> {
        self.agent_local_capabilities.get(server_id).map(|cap| *cap)
    }

    pub fn get_effective_capabilities(&self, server_id: &str) -> Option<u32> {
        self.get_server_capabilities(server_id)
            .zip(self.get_agent_local_capabilities(server_id))
            .map(|(server_caps, agent_local_caps)| {
                effective_capabilities(server_caps, agent_local_caps)
            })
    }

    pub fn capability_denied_reason(
        &self,
        server_id: &str,
        configured_caps: u32,
        cap_bit: u32,
    ) -> Option<&'static str> {
        if !has_capability(configured_caps, cap_bit) {
            Some("server_capability_disabled")
        } else if self
            .get_agent_local_capabilities(server_id)
            .is_some_and(|caps| !has_capability(caps, cap_bit))
        {
            Some("agent_capability_disabled")
        } else {
            None
        }
    }

    pub fn has_docker_capability(&self, server_id: &str) -> bool {
        self.get_effective_capabilities(server_id)
            .or_else(|| self.get_server_capabilities(server_id))
            .is_some_and(|cap| has_capability(cap, CAP_DOCKER))
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
            self.server_capabilities.insert(id.clone(), caps as u32);
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

    /// Remove pending requests that have exceeded their per-entry TTL.
    pub fn cleanup_expired_requests(&self) {
        let now = std::time::Instant::now();
        self.pending_requests
            .retain(|_, (_, created_at, ttl)| now.duration_since(*created_at) < *ttl);
    }

    // --- Traceroute result cache ---

    /// Insert a placeholder traceroute result entry (completed=false) before sending to agent.
    pub fn insert_traceroute_placeholder(&self, request_id: &str, server_id: &str, target: &str) {
        self.traceroute_results.insert(
            request_id.to_string(),
            TracerouteResultEntry {
                server_id: server_id.to_string(),
                result: TracerouteResultData {
                    target: target.to_string(),
                    hops: vec![],
                    completed: false,
                    error: None,
                },
                created_at: Instant::now(),
            },
        );
    }

    /// Update a traceroute result entry with the actual data from the agent.
    pub fn update_traceroute_result(&self, request_id: &str, result: TracerouteResultData) {
        if let Some(mut entry) = self.traceroute_results.get_mut(request_id) {
            entry.result = result;
        }
    }

    /// Get a traceroute result by request_id. Returns (server_id, result) clone.
    pub fn get_traceroute_result(
        &self,
        request_id: &str,
    ) -> Option<(String, TracerouteResultData)> {
        self.traceroute_results.get(request_id).map(|entry| {
            (
                entry.server_id.clone(),
                TracerouteResultData {
                    target: entry.result.target.clone(),
                    hops: entry.result.hops.clone(),
                    completed: entry.result.completed,
                    error: entry.result.error.clone(),
                },
            )
        })
    }

    /// Remove traceroute result entries older than 120 seconds.
    pub fn cleanup_traceroute_results(&self) {
        let now = Instant::now();
        self.traceroute_results
            .retain(|_, entry| now.duration_since(entry.created_at).as_secs() < 120);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::{CAP_DOCKER, CAP_EXEC, CAP_FILE};
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
    fn test_effective_capabilities_intersect_configured_and_local_caps() {
        let (mgr, _rx) = make_manager();
        mgr.update_server_capabilities("s1", CAP_EXEC | CAP_FILE);
        mgr.update_agent_local_capabilities("s1", CAP_FILE);

        assert_eq!(mgr.get_agent_local_capabilities("s1"), Some(CAP_FILE));
        assert_eq!(mgr.get_effective_capabilities("s1"), Some(CAP_FILE));
    }

    #[test]
    fn test_effective_capabilities_are_none_without_local_caps() {
        let (mgr, _rx) = make_manager();
        mgr.update_server_capabilities("s1", CAP_EXEC | CAP_FILE);

        assert_eq!(mgr.get_agent_local_capabilities("s1"), None);
        assert_eq!(mgr.get_effective_capabilities("s1"), None);
    }

    #[test]
    fn test_remove_connection_clears_runtime_capability_state() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());
        mgr.update_server_capabilities("s1", CAP_DOCKER);
        mgr.update_agent_local_capabilities("s1", CAP_DOCKER);

        assert!(mgr.has_docker_capability("s1"));

        mgr.remove_connection("s1");

        assert_eq!(mgr.get_agent_local_capabilities("s1"), None);
        assert_eq!(mgr.get_effective_capabilities("s1"), None);
        assert!(!mgr.has_docker_capability("s1"));
    }

    #[test]
    fn test_remove_connection_clears_docker_caches() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());

        mgr.update_docker_containers("s1", vec![]);
        mgr.update_docker_stats("s1", vec![]);
        mgr.update_docker_info(
            "s1",
            DockerSystemInfo {
                docker_version: "26.1.0".into(),
                api_version: "1.45".into(),
                os: "linux".into(),
                arch: "amd64".into(),
                containers_running: 1,
                containers_paused: 0,
                containers_stopped: 0,
                images: 1,
                memory_total: 1024,
            },
        );

        mgr.remove_connection("s1");

        assert!(mgr.get_docker_containers("s1").is_none());
        assert!(mgr.get_docker_stats("s1").is_none());
        assert!(mgr.get_docker_info("s1").is_none());
    }

    #[test]
    fn test_remove_connection_if_current_does_not_remove_newer_connection() {
        let (mgr, _rx) = make_manager();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);
        let first_connection_id = mgr.add_connection("s1".into(), "Srv".into(), tx1, test_addr());
        let second_connection_id = mgr.add_connection("s1".into(), "Srv".into(), tx2, test_addr());

        mgr.update_docker_containers("s1", vec![]);

        assert_ne!(first_connection_id, second_connection_id);
        assert!(!mgr.remove_connection_if_current("s1", first_connection_id));
        assert!(mgr.is_online("s1"));
        assert!(mgr.get_sender("s1").is_some());
        assert!(mgr.get_docker_containers("s1").is_some());
    }

    #[test]
    fn test_get_report_nonexistent() {
        let (mgr, _rx) = make_manager();
        assert!(mgr.get_latest_report("nope").is_none());
    }

    #[test]
    fn test_cleanup_expired_requests() {
        let (mgr, _rx) = make_manager();
        let _rx1 = mgr
            .register_pending_request_with_ttl("old".into(), std::time::Duration::from_millis(1));
        std::thread::sleep(std::time::Duration::from_millis(10));
        mgr.cleanup_expired_requests();
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

    #[test]
    fn test_cleanup_expired_requests_per_entry_ttl() {
        let (mgr, _rx) = make_manager();
        let _rx1 = mgr.register_pending_request_with_ttl(
            "short".into(),
            std::time::Duration::from_millis(10),
        );
        let _rx2 = mgr
            .register_pending_request_with_ttl("long".into(), std::time::Duration::from_secs(300));
        std::thread::sleep(std::time::Duration::from_millis(50));
        mgr.cleanup_expired_requests();
        assert!(!mgr.has_pending_request("short"));
        assert!(mgr.has_pending_request("long"));
    }
}
