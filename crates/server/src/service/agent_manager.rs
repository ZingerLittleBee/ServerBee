use std::net::SocketAddr;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};

use serverbee_common::protocol::{BrowserMessage, ServerMessage};
use serverbee_common::types::{ServerStatus, SystemReport};

/// Sender for forwarding terminal output from agent to browser WS.
pub type TerminalOutputTx = mpsc::Sender<TerminalSessionEvent>;

/// Events sent from agent handler to browser terminal WS.
pub enum TerminalSessionEvent {
    Output(String),  // base64 encoded data
    Started,
    Error(String),
}

pub struct AgentManager {
    connections: DashMap<String, AgentConnection>,
    latest_reports: DashMap<String, CachedReport>,
    browser_tx: broadcast::Sender<BrowserMessage>,
    /// Maps session_id -> terminal output channel (for routing agent output to browser WS)
    terminal_sessions: DashMap<String, TerminalOutputTx>,
}

#[allow(dead_code)]
pub struct AgentConnection {
    pub server_id: String,
    pub server_name: String,
    pub tx: mpsc::Sender<ServerMessage>,
    pub connected_at: Instant,
    pub last_report_at: Instant,
    pub remote_addr: SocketAddr,
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
            },
        );

        let _ = self.browser_tx.send(BrowserMessage::ServerOnline {
            server_id,
        });
    }

    /// Unregister an agent connection and broadcast ServerOffline to browsers.
    pub fn remove_connection(&self, server_id: &str) {
        self.connections.remove(server_id);

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
}
