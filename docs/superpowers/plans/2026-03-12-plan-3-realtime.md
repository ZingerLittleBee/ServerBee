# Plan 3: Real-time — Agent WebSocket Handler + AgentManager + Browser Push + Background Tasks

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the server-side real-time infrastructure: Agent WebSocket handler, AgentManager for connection/state management, Browser WebSocket push, and all background tasks (metric recording, aggregation, cleanup, alerting).

**Architecture:** AgentManager uses DashMap for concurrent state, broadcast channel for browser push. Background tasks run as tokio::spawn'd loops with cron-like scheduling.

**Tech Stack:** Rust, axum (WebSocket), tokio, dashmap, tokio-cron-scheduler

**Depends on:** Plan 1 (server skeleton + entities + services), Plan 2 (agent sending reports)

---

## Chunk 1: AgentManager + Agent WebSocket

### Task 1: AgentManager

**Files:**
- Create: `crates/server/src/service/agent_manager.rs`
- Modify: `crates/server/src/service/mod.rs`
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Write service/agent_manager.rs**

```rust
use std::net::SocketAddr;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};

use serverbee_common::protocol::{BrowserMessage, ServerMessage};
use serverbee_common::types::{ServerStatus, SystemReport};

pub struct AgentManager {
    connections: DashMap<String, AgentConnection>,
    latest_reports: DashMap<String, CachedReport>,
    browser_tx: broadcast::Sender<BrowserMessage>,
}

pub struct AgentConnection {
    pub server_id: String,
    pub server_name: String,
    pub tx: mpsc::Sender<ServerMessage>,
    pub connected_at: Instant,
    pub last_report_at: Instant,
    pub remote_addr: SocketAddr,
}

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
        }
    }

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

        let _ = self
            .browser_tx
            .send(BrowserMessage::ServerOnline { server_id });
    }

    pub fn remove_connection(&self, server_id: &str) {
        self.connections.remove(server_id);
        let _ = self.browser_tx.send(BrowserMessage::ServerOffline {
            server_id: server_id.to_string(),
        });
    }

    pub fn update_report(&self, server_id: &str, report: SystemReport) {
        // Update last_report_at
        if let Some(mut conn) = self.connections.get_mut(server_id) {
            conn.last_report_at = Instant::now();
        }

        // Cache latest report
        self.latest_reports.insert(
            server_id.to_string(),
            CachedReport {
                report,
                received_at: Instant::now(),
            },
        );

        // Broadcast to browsers
        let statuses = self.build_statuses_for(&[server_id.to_string()]);
        if !statuses.is_empty() {
            let _ = self.browser_tx.send(BrowserMessage::Update {
                servers: statuses,
            });
        }
    }

    pub fn is_online(&self, server_id: &str) -> bool {
        self.connections.contains_key(server_id)
    }

    pub fn online_count(&self) -> usize {
        self.connections.len()
    }

    pub fn get_latest_report(&self, server_id: &str) -> Option<SystemReport> {
        self.latest_reports
            .get(server_id)
            .map(|r| r.report.clone())
    }

    pub fn all_latest_reports(&self) -> Vec<(String, SystemReport)> {
        self.latest_reports
            .iter()
            .map(|r| (r.key().clone(), r.report.clone()))
            .collect()
    }

    pub fn get_sender(&self, server_id: &str) -> Option<mpsc::Sender<ServerMessage>> {
        self.connections.get(server_id).map(|c| c.tx.clone())
    }

    pub fn check_offline(&self, threshold_secs: u64) -> Vec<String> {
        let threshold = std::time::Duration::from_secs(threshold_secs);
        let mut offline = Vec::new();

        for entry in self.connections.iter() {
            if entry.last_report_at.elapsed() > threshold {
                offline.push(entry.server_id.clone());
            }
        }

        for id in &offline {
            self.remove_connection(id);
        }

        offline
    }

    pub fn build_full_sync(&self) -> Vec<ServerStatus> {
        // This needs DB data for static fields — will be called with pre-loaded server data
        Vec::new()
    }

    fn build_statuses_for(&self, server_ids: &[String]) -> Vec<ServerStatus> {
        server_ids
            .iter()
            .filter_map(|id| {
                let report = self.latest_reports.get(id)?;
                let conn = self.connections.get(id)?;
                Some(ServerStatus {
                    id: id.clone(),
                    name: conn.server_name.clone(),
                    online: true,
                    last_active: report.received_at.elapsed().as_secs() as i64,
                    uptime: report.report.uptime,
                    cpu: report.report.cpu,
                    mem_used: report.report.mem_used,
                    mem_total: 0, // Static, needs DB
                    swap_used: report.report.swap_used,
                    swap_total: 0,
                    disk_used: report.report.disk_used,
                    disk_total: 0,
                    net_in_speed: report.report.net_in_speed,
                    net_out_speed: report.report.net_out_speed,
                    net_in_transfer: report.report.net_in_transfer,
                    net_out_transfer: report.report.net_out_transfer,
                    load1: report.report.load1,
                    load5: report.report.load5,
                    load15: report.report.load15,
                    tcp_conn: report.report.tcp_conn,
                    udp_conn: report.report.udp_conn,
                    process_count: report.report.process_count,
                    cpu_name: None,
                    os: None,
                    region: None,
                    country_code: None,
                })
            })
            .collect()
    }
}
```

- [ ] **Step 2: Add AgentManager to AppState**

Update `state.rs` to include `agent_manager: AgentManager` in AppState and initialize it in `new()`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/agent_manager.rs crates/server/src/state.rs
git commit -m "feat(server): implement AgentManager for connection and state management"
```

### Task 2: Agent WebSocket Handler

**Files:**
- Create: `crates/server/src/router/ws/mod.rs`
- Create: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/router/mod.rs`

- [ ] **Step 1: Create router/ws/mod.rs**

```rust
pub mod agent;
pub mod browser;
```

- [ ] **Step 2: Write router/ws/agent.rs**

Implements the `GET /api/agent/ws?token=xxx` handler:

1. Extract token from query params
2. Validate agent token against DB (using token_prefix for lookup, then argon2 verify)
3. Upgrade to WebSocket
4. Send `Welcome` message
5. Create mpsc channel, register in AgentManager
6. Loop: receive AgentMessage, dispatch accordingly:
   - `SystemInfo` → update server record in DB, send `Ack`
   - `Report` → call `agent_manager.update_report()`
   - `TaskResult` → store in DB, send `Ack`
   - `PingResult` → store in DB
   - `Pong` → update heartbeat
7. On disconnect → remove from AgentManager
8. Spawn a task to forward ServerMessage from mpsc channel to WebSocket write

- [ ] **Step 3: Add agent WebSocket route to router/mod.rs**

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/ws/
git commit -m "feat(server): implement Agent WebSocket handler"
```

### Task 3: Browser WebSocket Handler

**Files:**
- Create: `crates/server/src/router/ws/browser.rs`
- Modify: `crates/server/src/router/mod.rs`

- [ ] **Step 1: Write router/ws/browser.rs**

Implements `GET /api/ws/servers`:

1. Validate session cookie or API key
2. Upgrade to WebSocket
3. Build and send `FullSync` with all current server statuses
4. Subscribe to `browser_tx` broadcast channel
5. Forward all `BrowserMessage` to WebSocket
6. On client disconnect, drop subscription

- [ ] **Step 2: Add browser WebSocket route**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/ws/browser.rs
git commit -m "feat(server): implement Browser WebSocket handler for real-time updates"
```

## Chunk 2: Background Tasks

### Task 4: Metric Recording Task

**Files:**
- Create: `crates/server/src/task/mod.rs`
- Create: `crates/server/src/task/record_writer.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Create task/mod.rs**

```rust
pub mod record_writer;
pub mod aggregator;
pub mod cleanup;
pub mod offline_checker;
pub mod session_cleaner;
```

- [ ] **Step 2: Write task/record_writer.rs**

Every 60 seconds:
1. Get all latest_reports from AgentManager
2. For each server with a report, insert one record into `records` table
3. If GPU data present, insert into `gpu_records` table
4. Log count of records written

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::service::record::RecordService;
use crate::state::AppState;

pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        let reports = state.agent_manager.all_latest_reports();
        if reports.is_empty() {
            continue;
        }

        let mut count = 0;
        for (server_id, report) in &reports {
            if let Err(e) = RecordService::save_report(&state.db, server_id, report).await {
                tracing::error!("Failed to save record for {server_id}: {e}");
            } else {
                count += 1;
            }
        }

        tracing::debug!("Wrote {count} metric records");
    }
}
```

- [ ] **Step 3: Spawn record_writer in main.rs**

Add `tokio::spawn(task::record_writer::run(state.clone()));` before starting the HTTP server.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/task/
git commit -m "feat(server): add background metric recording task (1min interval)"
```

### Task 5: Hourly Aggregation Task

**Files:**
- Create: `crates/server/src/task/aggregator.rs`

- [ ] **Step 1: Write task/aggregator.rs**

Every 3600 seconds (1 hour):
1. Call `RecordService::aggregate_hourly()` — queries records from last hour, computes averages per server, inserts into `records_hourly`
2. Log aggregation count

- [ ] **Step 2: Spawn in main.rs**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/aggregator.rs
git commit -m "feat(server): add hourly metric aggregation task"
```

### Task 6: Data Cleanup Task

**Files:**
- Create: `crates/server/src/task/cleanup.rs`

- [ ] **Step 1: Write task/cleanup.rs**

Runs after aggregation (same hourly interval, offset by a few seconds):
1. Delete records older than `retention.records_days`
2. Delete records_hourly older than `retention.records_hourly_days`
3. Delete gpu_records older than `retention.gpu_records_days`
4. Delete ping_records older than `retention.ping_records_days`
5. Delete audit_logs older than `retention.audit_logs_days`
6. Log deletion counts

- [ ] **Step 2: Spawn in main.rs**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/cleanup.rs
git commit -m "feat(server): add data cleanup task with configurable retention"
```

### Task 7: Offline Detection Task

**Files:**
- Create: `crates/server/src/task/offline_checker.rs`

- [ ] **Step 1: Write task/offline_checker.rs**

Every 10 seconds:
1. Call `agent_manager.check_offline(30)` — removes agents with no report for 30s
2. For each offline agent, the AgentManager already broadcasts `ServerOffline`
3. Log offline events

- [ ] **Step 2: Spawn in main.rs**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/offline_checker.rs
git commit -m "feat(server): add offline detection task (10s scan interval)"
```

### Task 8: Session Cleanup Task

**Files:**
- Create: `crates/server/src/task/session_cleaner.rs`

- [ ] **Step 1: Write task/session_cleaner.rs**

Every 3600 seconds:
1. Delete sessions where `expires_at < now()`
2. Log count deleted

- [ ] **Step 2: Spawn in main.rs**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/session_cleaner.rs
git commit -m "feat(server): add expired session cleanup task"
```

## Chunk 3: Heartbeat + End-to-End Test

### Task 9: Server-side Heartbeat

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Add heartbeat Ping to agent WebSocket handler**

In the agent handler's forwarding task, add a 30-second interval that sends `ServerMessage::Ping` to the agent. If the agent doesn't respond with `Pong` within the threshold, the connection is considered dead.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/ws/agent.rs
git commit -m "feat(server): add heartbeat ping to agent WebSocket connections"
```

### Task 10: End-to-End Integration Test

**Files:** No new files — manual verification

- [ ] **Step 1: Start server**

Run: `cargo run -p serverbee-server`
Expected: Server starts, creates DB, logs admin credentials and discovery key.

- [ ] **Step 2: Start agent**

Create `agent.toml` with server_url and auto_discovery_key.
Run: `cargo run -p serverbee-agent`
Expected: Agent registers, connects, sends SystemInfo, starts reporting every 3s.

- [ ] **Step 3: Verify real-time data flow**

1. Login via `POST /api/auth/login`
2. Check `GET /api/servers` — should show the registered agent with static info
3. Connect to `ws://localhost:9527/api/ws/servers` — should receive FullSync and periodic Updates
4. Wait 1 minute — check `GET /api/servers/:id/records` returns metric data

- [ ] **Step 4: Verify offline detection**

Stop the agent process. Wait 30s. Check server logs for offline detection. Browser WebSocket should receive `ServerOffline` message.

- [ ] **Step 5: Commit test notes (if any fixes needed)**

```bash
git add -u
git commit -m "fix: integration fixes from end-to-end testing"
```
