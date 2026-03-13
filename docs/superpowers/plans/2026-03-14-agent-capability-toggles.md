# Agent Capability Toggles Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-agent feature toggles (bitmap) with dual validation (server blocks + agent refuses) so admins can independently enable/disable dangerous remote operations per server.

**Architecture:** u32 bitmap stored in `servers.capabilities` column. Server checks capability before dispatching; Agent checks before executing. Changes sync via WebSocket `CapabilitiesSync` message (protocol v2+ agents only). Frontend uses REST for initial data and WS for real-time updates across all query caches.

**Tech Stack:** Rust (sea-orm migration, Axum handlers, AtomicU32), React (TanStack Query cache sync, Toggle components)

**Spec:** `docs/superpowers/specs/2026-03-14-agent-capability-toggles-design.md`

---

## Chunk 1: Common Crate + Database Migration

### Task 1: Add capability constants and metadata to common crate

**Files:**
- Modify: `crates/common/src/constants.rs`

- [ ] **Step 1: Add capability bit constants**

Add after existing constants in `crates/common/src/constants.rs`:

```rust
// --- Capability Toggles ---

pub const CAP_TERMINAL: u32 = 1 << 0; // 1
pub const CAP_EXEC: u32 = 1 << 1; // 2
pub const CAP_UPGRADE: u32 = 1 << 2; // 4
pub const CAP_PING_ICMP: u32 = 1 << 3; // 8
pub const CAP_PING_TCP: u32 = 1 << 4; // 16
pub const CAP_PING_HTTP: u32 = 1 << 5; // 32

pub const CAP_DEFAULT: u32 = CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP; // 56
pub const CAP_VALID_MASK: u32 = 0b0011_1111; // 63

#[derive(Debug)]
pub struct CapabilityMeta {
    pub bit: u32,
    pub key: &'static str,
    pub display_name: &'static str,
    pub default_enabled: bool,
    pub risk_level: &'static str,
}

pub const ALL_CAPABILITIES: &[CapabilityMeta] = &[
    CapabilityMeta { bit: CAP_TERMINAL, key: "terminal", display_name: "Web Terminal", default_enabled: false, risk_level: "high" },
    CapabilityMeta { bit: CAP_EXEC, key: "exec", display_name: "Remote Exec", default_enabled: false, risk_level: "high" },
    CapabilityMeta { bit: CAP_UPGRADE, key: "upgrade", display_name: "Auto Upgrade", default_enabled: false, risk_level: "high" },
    CapabilityMeta { bit: CAP_PING_ICMP, key: "ping_icmp", display_name: "ICMP Ping", default_enabled: true, risk_level: "low" },
    CapabilityMeta { bit: CAP_PING_TCP, key: "ping_tcp", display_name: "TCP Probe", default_enabled: true, risk_level: "low" },
    CapabilityMeta { bit: CAP_PING_HTTP, key: "ping_http", display_name: "HTTP Probe", default_enabled: true, risk_level: "low" },
];

/// Check if a specific capability bit is set.
pub fn has_capability(capabilities: u32, cap_bit: u32) -> bool {
    capabilities & cap_bit != 0
}

/// Map probe_type string to capability bit.
pub fn probe_type_to_cap(probe_type: &str) -> Option<u32> {
    match probe_type {
        "icmp" => Some(CAP_PING_ICMP),
        "tcp" => Some(CAP_PING_TCP),
        "http" => Some(CAP_PING_HTTP),
        _ => None,
    }
}
```

- [ ] **Step 2: Update PROTOCOL_VERSION**

In `crates/common/src/constants.rs`, change:
```rust
// Before
pub const PROTOCOL_VERSION: u32 = 1;
// After
pub const PROTOCOL_VERSION: u32 = 2;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-common`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/common/src/constants.rs
git commit -m "feat(common): add capability bitmap constants and helpers"
```

---

### Task 2: Extend protocol messages

**Files:**
- Modify: `crates/common/src/protocol.rs`
- Modify: `crates/common/src/types.rs`

- [ ] **Step 1: Add protocol_version to SystemInfo**

In `crates/common/src/types.rs`, add the default function immediately after the `SystemInfo` struct, then add the field to the struct:

```rust
fn default_protocol_version() -> u32 {
    1
}
```

Add field at end of `SystemInfo` struct (after `agent_version`):
```rust
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u32,
```

- [ ] **Step 2: Add capabilities to Welcome variant**

In `crates/common/src/protocol.rs`, modify the existing `Welcome` variant (lines 39-43) to add the `capabilities` field:

Before:
```rust
    Welcome {
        server_id: String,
        protocol_version: u32,
        report_interval: u32,
    },
```

After:
```rust
    Welcome {
        server_id: String,
        protocol_version: u32,
        report_interval: u32,
        #[serde(default)]
        capabilities: Option<u32>,
    },
```

- [ ] **Step 3: Add CapabilitiesSync variant to ServerMessage**

Add after the `Upgrade` variant (line 76):
```rust
    CapabilitiesSync {
        capabilities: u32,
    },
```

- [ ] **Step 4: Add CapabilityDenied to AgentMessage**

Add before the `Pong` variant (line 32):
```rust
    CapabilityDenied {
        msg_id: Option<String>,
        session_id: Option<String>,
        capability: String,
    },
```

- [ ] **Step 5: Add CapabilitiesChanged and AgentInfoUpdated to BrowserMessage**

Add after the `ServerOffline` variant (line 94):
```rust
    CapabilitiesChanged {
        server_id: String,
        capabilities: u32,
    },
    AgentInfoUpdated {
        server_id: String,
        protocol_version: u32,
    },
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p serverbee-common`
Expected: compiles with no errors

- [ ] **Step 7: Commit**

```bash
git add crates/common/src/protocol.rs crates/common/src/types.rs
git commit -m "feat(common): extend protocol with capability messages and version negotiation"
```

---

### Task 3: Database migration + entity update

**Files:**
- Create: `crates/server/src/migration/m20260314_000003_add_capabilities.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Modify: `crates/server/src/entity/server.rs`

- [ ] **Step 1: Create migration file**

Create `crates/server/src/migration/m20260314_000003_add_capabilities.rs`:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260314_000003_add_capabilities"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN capabilities INTEGER NOT NULL DEFAULT 56"
        ).await?;
        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT 1"
        ).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite does not support DROP COLUMN in older versions;
        // for simplicity, this migration is not reversible.
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration**

Replace `crates/server/src/migration/mod.rs` with:

```rust
use sea_orm_migration::prelude::*;

mod m20260312_000001_init;
mod m20260312_000002_oauth;
mod m20260314_000003_add_capabilities;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260312_000001_init::Migration),
            Box::new(m20260312_000002_oauth::Migration),
            Box::new(m20260314_000003_add_capabilities::Migration),
        ]
    }
}
```

- [ ] **Step 3: Update server entity**

In `crates/server/src/entity/server.rs`, add these two fields before `created_at` (after `traffic_limit_type` on line 37):

```rust
    pub capabilities: i32,
    pub protocol_version: i32,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration/ crates/server/src/entity/server.rs
git commit -m "feat(server): add capabilities and protocol_version columns via migration"
```

---

## Chunk 2: Server-Side Enforcement

**Prerequisites from Chunk 1:** `CAP_*` constants, `CAP_VALID_MASK`, `has_capability()`, `probe_type_to_cap()` in common crate; `ServerMessage::CapabilitiesSync`, `Welcome.capabilities`, `AgentMessage::CapabilityDenied`, `BrowserMessage::CapabilitiesChanged/AgentInfoUpdated` in protocol; `server::Model` with `capabilities` and `protocol_version` fields.

### Task 4: AgentManager — protocol_version tracking + browser_tx accessor

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs`

- [ ] **Step 1: Add protocol_version to AgentConnection**

In `AgentConnection` struct (line 29-36), add after `remote_addr`:
```rust
    pub protocol_version: u32,
```

In `add_connection()` (line 65), add to the `AgentConnection` construction:
```rust
                protocol_version: 1,
```

- [ ] **Step 2: Add protocol_version methods**

Add to `impl AgentManager`:
```rust
    pub fn set_protocol_version(&self, server_id: &str, version: u32) {
        if let Some(mut conn) = self.connections.get_mut(server_id) {
            conn.protocol_version = version;
        }
    }

    pub fn get_protocol_version(&self, server_id: &str) -> Option<u32> {
        self.connections.get(server_id).map(|c| c.protocol_version)
    }
```

- [ ] **Step 3: Add browser_tx accessor**

Add to `impl AgentManager`:
```rust
    pub fn broadcast_browser(&self, msg: BrowserMessage) {
        let _ = self.browser_tx.send(msg);
    }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/agent_manager.rs
git commit -m "feat(server): track agent protocol_version, add browser broadcast accessor"
```

---

### Task 5: Modify AppError::Forbidden to accept message

**Files:**
- Modify: `crates/server/src/error.rs`

- [ ] **Step 1: Update Forbidden variant**

Change `AppError::Forbidden` from unit variant to tuple variant:

Before (lines 39-41):
```rust
    #[error("Forbidden")]
    #[allow(dead_code)]
    Forbidden,
```

After:
```rust
    #[error("Forbidden: {0}")]
    Forbidden(String),
```

Update the `IntoResponse` match arm (line 59):
```rust
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles (no existing code uses `AppError::Forbidden`)

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/error.rs
git commit -m "refactor(server): make AppError::Forbidden accept a message string"
```

---

### Task 6: Server API — capabilities in ServerResponse + batch endpoint

**Files:**
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/service/server.rs`

- [ ] **Step 1: Add capabilities and protocol_version to ServerResponse**

In `crates/server/src/router/api/server.rs`, add to `ServerResponse` struct:
```rust
    pub capabilities: i32,
    pub protocol_version: i32,
```

Update `From<server::Model>` impl to map both fields:
```rust
            capabilities: s.capabilities,
            protocol_version: s.protocol_version,
```

- [ ] **Step 2: Add capabilities to UpdateServerInput**

In `crates/server/src/service/server.rs`, add to `UpdateServerInput` (after `traffic_limit_type`):
```rust
    pub capabilities: Option<i32>,
```

In `update_server()`, add after the `traffic_limit_type` handling (before `active.updated_at`):
```rust
        if let Some(caps) = input.capabilities {
            let caps_u32 = caps as u32;
            if caps_u32 & !serverbee_common::constants::CAP_VALID_MASK != 0 {
                return Err(AppError::Validation("Invalid capability bits".into()));
            }
            active.capabilities = Set(caps);
        }
```

- [ ] **Step 3: Add batch_update_capabilities endpoint**

In `crates/server/src/router/api/server.rs`, add request/response types:

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct BatchCapabilitiesRequest {
    server_ids: Vec<String>,
    #[serde(default)]
    set: u32,
    #[serde(default)]
    unset: u32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct BatchCapabilitiesResponse {
    updated: u64,
}
```

Add handler:
```rust
async fn batch_update_capabilities(
    State(state): State<Arc<AppState>>,
    Extension((user_id, _role, ip)): Extension<(String, String, String)>,
    Json(input): Json<BatchCapabilitiesRequest>,
) -> Result<Json<ApiResponse<BatchCapabilitiesResponse>>, AppError> {
    use serverbee_common::constants::*;

    // Validate bits within mask
    if input.set & !CAP_VALID_MASK != 0 || input.unset & !CAP_VALID_MASK != 0 {
        return Err(AppError::Validation("Invalid capability bits".into()));
    }
    // No overlap
    if input.set & input.unset != 0 {
        return Err(AppError::Validation("set and unset must not overlap".into()));
    }
    if input.server_ids.is_empty() {
        return ok(BatchCapabilitiesResponse { updated: 0 });
    }

    let servers = server::Entity::find()
        .filter(server::Column::Id.is_in(input.server_ids.iter().cloned()))
        .all(&*state.db)
        .await?;

    let mut count = 0u64;
    for s in &servers {
        let old_caps = s.capabilities as u32;
        let new_caps = (old_caps & !input.unset) | input.set;
        if new_caps == old_caps {
            continue;
        }

        let mut active: server::ActiveModel = s.clone().into();
        active.capabilities = Set(new_caps as i32);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&*state.db).await?;
        count += 1;

        // Sync to agent if online and protocol v2+
        if let Some(pv) = state.agent_manager.get_protocol_version(&s.id) {
            if pv >= 2 {
                if let Some(tx) = state.agent_manager.get_sender(&s.id) {
                    let _ = tx.send(ServerMessage::CapabilitiesSync { capabilities: new_caps }).await;
                }
            }
        }

        // Broadcast to browsers
        state.agent_manager.broadcast_browser(BrowserMessage::CapabilitiesChanged {
            server_id: s.id.clone(),
            capabilities: new_caps,
        });

        // Re-sync ping tasks if ping bits changed
        let ping_mask = CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP;
        if old_caps & ping_mask != new_caps & ping_mask {
            PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, &s.id).await;
        }

        // Audit log
        let detail = serde_json::json!({
            "server_id": s.id,
            "old": old_caps,
            "new": new_caps,
        }).to_string();
        let _ = AuditService::log(&state.db, &user_id, "capabilities_changed", Some(&detail), &ip).await;
    }

    ok(BatchCapabilitiesResponse { updated: count })
}
```

Register route in `write_router()`:
```rust
.route("/servers/batch-capabilities", put(batch_update_capabilities))
```

- [ ] **Step 4: Add capability check to trigger_upgrade**

In `trigger_upgrade()`, before sending Upgrade message:
```rust
let server = ServerService::get_server(&state.db, &server_id).await?;
if !has_capability(server.capabilities as u32, CAP_UPGRADE) {
    return Err(AppError::Forbidden("Upgrade is disabled for this server".into()));
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/api/server.rs crates/server/src/service/server.rs
git commit -m "feat(server): add capabilities to API responses and batch update endpoint"
```

---

### Task 7: Agent WS handler — Welcome with capabilities + AgentInfoUpdated

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/service/server.rs`

- [ ] **Step 1: Send capabilities in Welcome message**

In `handle_agent_ws()`, the `server` variable is already available from the auth check. Modify Welcome construction (lines 74-78):

Before:
```rust
    let welcome = ServerMessage::Welcome {
        server_id: server_id.clone(),
        protocol_version: 1,
        report_interval: 3,
    };
```

After:
```rust
    let welcome = ServerMessage::Welcome {
        server_id: server_id.clone(),
        protocol_version: serverbee_common::constants::PROTOCOL_VERSION,
        report_interval: 3,
        capabilities: Some(server.capabilities as u32),
    };
```

- [ ] **Step 2: Handle SystemInfo — persist protocol_version + broadcast AgentInfoUpdated**

In `handle_agent_message`, in the `AgentMessage::SystemInfo` branch, after the `update_system_info` call (after line 197), add:

```rust
            // Update in-memory protocol_version
            let agent_pv = info.protocol_version;
            state.agent_manager.set_protocol_version(server_id, agent_pv);

            // Broadcast to browsers
            state.agent_manager.broadcast_browser(BrowserMessage::AgentInfoUpdated {
                server_id: server_id.to_string(),
                protocol_version: agent_pv,
            });
```

- [ ] **Step 3: Update ServerService::update_system_info to persist protocol_version**

In `crates/server/src/service/server.rs`, in `update_system_info()`, add before `active.updated_at` (line 159):
```rust
        active.protocol_version = Set(info.protocol_version as i32);
```

- [ ] **Step 4: Handle CapabilityDenied from Agent**

Add match arm in `handle_agent_message` (after `AgentMessage::Pong`):
```rust
        AgentMessage::CapabilityDenied { msg_id, session_id, capability } => {
            tracing::warn!(
                "Agent {server_id} denied capability '{capability}' (msg_id={msg_id:?}, session_id={session_id:?})"
            );
            // For terminal: unregister session
            if let Some(sid) = &session_id {
                state.agent_manager.unregister_terminal_session(sid);
            }
        }
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/ws/agent.rs crates/server/src/service/server.rs
git commit -m "feat(server): send capabilities in Welcome, handle CapabilityDenied, broadcast AgentInfoUpdated"
```

---

### Task 8: Terminal WS capability check

**Files:**
- Modify: `crates/server/src/router/ws/terminal.rs`

- [ ] **Step 1: Add CAP_TERMINAL check**

In `terminal_ws_handler()`, after the online check (after line 43), add:
```rust
            // Check terminal capability
            let server = ServerService::get_server(&state.db, &server_id).await
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response())?;
            if !serverbee_common::constants::has_capability(server.capabilities as u32, serverbee_common::constants::CAP_TERMINAL) {
                return (
                    axum::http::StatusCode::FORBIDDEN,
                    "Terminal is disabled for this server",
                ).into_response();
            }
```

Note: Need to import `crate::service::server::ServerService` at top of file.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/ws/terminal.rs
git commit -m "feat(server): block terminal WS if CAP_TERMINAL disabled"
```

---

### Task 9: Task API — filter by CAP_EXEC + synthetic results

**Files:**
- Modify: `crates/server/src/router/api/task.rs`

- [ ] **Step 1: Partition servers by capability in create_task**

In `create_task()`, after validation, before dispatching to agents:
```rust
    use serverbee_common::constants::{has_capability, CAP_EXEC};

    // Fetch capabilities for all target servers
    let servers = server::Entity::find()
        .filter(server::Column::Id.is_in(input.server_ids.iter().cloned()))
        .all(&*state.db)
        .await?;

    let (capable, disabled): (Vec<_>, Vec<_>) = input.server_ids.iter().partition(|sid| {
        servers.iter()
            .find(|s| &s.id == *sid)
            .map(|s| has_capability(s.capabilities as u32, CAP_EXEC))
            .unwrap_or(false)
    });

    // Write synthetic results for disabled servers
    for sid in &disabled {
        let result = task_result::ActiveModel {
            id: NotSet,
            task_id: Set(task_id.clone()),
            server_id: Set(sid.to_string()),
            output: Set("Capability 'exec' is disabled for this server".to_string()),
            exit_code: Set(-2),
            finished_at: Set(chrono::Utc::now()),
        };
        result.insert(&*state.db).await?;
    }

    // Modify existing dispatch loop to iterate over `capable` instead of `input.server_ids`
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/api/task.rs
git commit -m "feat(server): filter tasks by CAP_EXEC, write synthetic results for disabled servers"
```

---

### Task 10: Ping service — filter by capability

**Files:**
- Modify: `crates/server/src/service/ping.rs`

- [ ] **Step 1: Add capability filter to sync_tasks_to_agent**

In `sync_tasks_to_agent()`, after building `task_configs`, add filter by capability:
```rust
    use serverbee_common::constants::{has_capability, probe_type_to_cap, CAP_DEFAULT};

    // Fetch server capabilities
    let server_caps = server::Entity::find_by_id(server_id)
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|s| s.capabilities as u32)
        .unwrap_or(CAP_DEFAULT);

    let task_configs: Vec<PingTaskConfig> = task_configs
        .into_iter()
        .filter(|t| {
            probe_type_to_cap(&t.probe_type)
                .map(|cap| has_capability(server_caps, cap))
                .unwrap_or(false)
        })
        .collect();
```

Also modify the send logic to **always** send PingTasksSync (even if empty):
```rust
    // Always send PingTasksSync (even if empty — tells Agent to stop all probes)
    if let Some(tx) = agent_manager.get_sender(server_id) {
        let msg = ServerMessage::PingTasksSync { tasks: task_configs };
        let _ = tx.send(msg).await;
    }
```

- [ ] **Step 2: Apply same filter in sync_tasks_to_agents (batch)**

In `sync_tasks_to_agents()`, apply the same capability filter per agent when building per-agent task lists. Fetch each server's capabilities and filter by `probe_type_to_cap`. Ensure every connected agent gets a `PingTasksSync` (even empty).

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/ping.rs
git commit -m "feat(server): filter ping tasks by CAP_PING_* capability"
```

---

### Task 11: Capabilities change side-effects in update_server handler

**Files:**
- Modify: `crates/server/src/router/api/server.rs`

- [ ] **Step 1: Add capabilities change side-effects in update_server handler**

In the `update_server` API handler (not the service method), after calling `ServerService::update_server()`, add side-effects:

```rust
    // If capabilities changed, broadcast + re-sync
    if input.capabilities.is_some() {
        let updated = ServerService::get_server(&state.db, &id).await?;
        let new_caps = updated.capabilities as u32;

        // Send CapabilitiesSync to Agent (if online and protocol_version >= 2)
        if let Some(pv) = state.agent_manager.get_protocol_version(&id) {
            if pv >= 2 {
                if let Some(tx) = state.agent_manager.get_sender(&id) {
                    let _ = tx.send(ServerMessage::CapabilitiesSync { capabilities: new_caps }).await;
                }
            }
        }

        // Broadcast to browsers
        state.agent_manager.broadcast_browser(BrowserMessage::CapabilitiesChanged {
            server_id: id.clone(),
            capabilities: new_caps,
        });

        // Re-sync ping tasks if ping bits changed
        PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, &id).await;
    }
```

Note: The API handler already has `State(state)` and auth context. No need to change `ServerService::update_server` signature — keep the service method as pure DB operation, side-effects live in the handler.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/api/server.rs
git commit -m "feat(server): broadcast capabilities changes and re-sync pings from update handler"
```

---

## Chunk 3: Agent-Side Enforcement

**Prerequisites from Chunks 1-2:** `CAP_*` constants, `has_capability()`, `probe_type_to_cap()`, `PROTOCOL_VERSION` (= 2) in common crate; `ServerMessage::CapabilitiesSync`, `Welcome.capabilities: Option<u32>`, `AgentMessage::CapabilityDenied`, `SystemInfo.protocol_version: u32` (with `#[serde(default)]`).

### Task 12: Agent reporter — capabilities handling

**Files:**
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Add capabilities as local variable in connect_and_report**

The `capabilities` must be a local `Arc<AtomicU32>` inside `connect_and_report()` (not a struct field), because on reconnect it must reset to `u32::MAX`.

At the top of `connect_and_report()`, add:
```rust
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use serverbee_common::constants::*;

    let capabilities = Arc::new(AtomicU32::new(u32::MAX));
```

- [ ] **Step 2: Parse capabilities from Welcome**

Modify the Welcome match (lines 64-72) to extract the new `capabilities` field:

Before:
```rust
                    ServerMessage::Welcome {
                        server_id,
                        report_interval,
                        ..
                    } => {
                        tracing::info!("Welcome from server {server_id}, interval={report_interval}s");
                        report_interval
                    }
```

After:
```rust
                    ServerMessage::Welcome {
                        server_id,
                        report_interval,
                        capabilities: caps,
                        ..
                    } => {
                        tracing::info!("Welcome from server {server_id}, interval={report_interval}s");
                        if let Some(c) = caps {
                            capabilities.store(c, Ordering::SeqCst);
                        } else {
                            capabilities.store(u32::MAX, Ordering::SeqCst);
                        }
                        report_interval
                    }
```

- [ ] **Step 3: Pass capabilities to PingManager and TerminalManager**

Modify the constructor calls (around lines 103-107):

Before:
```rust
        let mut ping_manager = PingManager::new(ping_tx);
        // ...
        let mut terminal_manager = TerminalManager::new(term_tx);
```

After:
```rust
        let mut ping_manager = PingManager::new(ping_tx, Arc::clone(&capabilities));
        // ...
        let mut terminal_manager = TerminalManager::new(term_tx, Arc::clone(&capabilities));
```

- [ ] **Step 4: Update handle_server_message signature**

Add `capabilities: &Arc<AtomicU32>` parameter to `handle_server_message`:

Before:
```rust
    async fn handle_server_message<S>(
        &self,
        text: &str,
        write: &mut S,
        ping_manager: &mut PingManager,
        terminal_manager: &mut TerminalManager,
        cmd_result_tx: &mpsc::Sender<AgentMessage>,
    ) -> anyhow::Result<()>
```

After:
```rust
    async fn handle_server_message<S>(
        &self,
        text: &str,
        write: &mut S,
        ping_manager: &mut PingManager,
        terminal_manager: &mut TerminalManager,
        cmd_result_tx: &mpsc::Sender<AgentMessage>,
        capabilities: &Arc<AtomicU32>,
    ) -> anyhow::Result<()>
```

Update the call site (line 161) to pass `&capabilities`.

- [ ] **Step 5: Handle CapabilitiesSync message**

Add match arm in `handle_server_message`:
```rust
            ServerMessage::CapabilitiesSync { capabilities: caps } => {
                tracing::info!("Capabilities updated: {caps}");
                capabilities.store(caps, Ordering::SeqCst);
            }
```

- [ ] **Step 6: Check CAP_EXEC before executing commands**

In the `Exec` handler, before `tracing::info!("Executing command...")` (line 221), add:
```rust
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_EXEC) {
                    tracing::warn!("Exec denied: capability disabled (task_id={task_id})");
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: Some(task_id),
                        session_id: None,
                        capability: "exec".to_string(),
                    };
                    let tx = cmd_result_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(denied).await;
                    });
                    return Ok(());
                }
```

- [ ] **Step 7: Check CAP_UPGRADE before upgrading**

In the `Upgrade` handler, before `tracing::info!("Upgrade requested...")` (line 273), add:
```rust
                let caps = capabilities.load(Ordering::SeqCst);
                if !has_capability(caps, CAP_UPGRADE) {
                    tracing::warn!("Upgrade denied: capability disabled");
                    let denied = AgentMessage::CapabilityDenied {
                        msg_id: None,
                        session_id: None,
                        capability: "upgrade".to_string(),
                    };
                    let json = serde_json::to_string(&denied)?;
                    write.send(Message::Text(json.into())).await?;
                    return Ok(());
                }
```

- [ ] **Step 8: Send protocol_version in SystemInfo**

When building the `SystemInfo` message (lines 92-96), set `protocol_version`:
```rust
        let info = collector.system_info();
        let info_msg = AgentMessage::SystemInfo {
            msg_id: uuid::Uuid::new_v4().to_string(),
            info: SystemInfo {
                protocol_version: PROTOCOL_VERSION,
                ..info
            },
        };
```

Or if SystemInfo is constructed by `collector.system_info()`, set `protocol_version` on the returned struct before wrapping.

- [ ] **Step 9: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles

- [ ] **Step 10: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): enforce capabilities on Exec, Upgrade, CapabilitiesSync, and version reporting"
```

---

### Task 13: Agent pinger — filter by capability

**Files:**
- Modify: `crates/agent/src/pinger.rs`

- [ ] **Step 1: Add capabilities to PingManager**

Add field to `PingManager` struct:
```rust
    capabilities: Arc<AtomicU32>,
```

Update constructor:
```rust
    pub fn new(result_tx: mpsc::Sender<PingResult>, capabilities: Arc<AtomicU32>) -> Self {
        Self {
            tasks: HashMap::new(),
            result_tx,
            capabilities,
        }
    }
```

Add imports at top:
```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use serverbee_common::constants::{has_capability, probe_type_to_cap};
```

- [ ] **Step 2: Filter tasks in sync()**

In `sync()`, after receiving `configs`, filter before processing:
```rust
        let caps = self.capabilities.load(Ordering::SeqCst);
        let configs: Vec<_> = configs.into_iter().filter(|c| {
            probe_type_to_cap(&c.probe_type)
                .map(|cap| has_capability(caps, cap))
                .unwrap_or(false)
        }).collect();
```

Use this filtered `configs` for the rest of `sync()`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/pinger.rs
git commit -m "feat(agent): filter ping tasks by capability bitmap"
```

---

### Task 14: Agent terminal — check CAP_TERMINAL

**Files:**
- Modify: `crates/agent/src/terminal.rs`

- [ ] **Step 1: Add capabilities to TerminalManager**

Add field to `TerminalManager` struct:
```rust
    capabilities: Arc<AtomicU32>,
```

Update constructor:
```rust
    pub fn new(event_tx: mpsc::Sender<TerminalEvent>, capabilities: Arc<AtomicU32>) -> Self {
        Self {
            sessions: HashMap::new(),
            event_tx,
            capabilities,
        }
    }
```

Add imports at top:
```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use serverbee_common::constants::{has_capability, CAP_TERMINAL};
```

- [ ] **Step 2: Check capability in open()**

At the very start of `open()`, before the max sessions check (line 41):
```rust
        let caps = self.capabilities.load(Ordering::SeqCst);
        if !has_capability(caps, CAP_TERMINAL) {
            tracing::warn!("Terminal denied: capability disabled (session={session_id})");
            let tx = self.event_tx.clone();
            let sid = session_id;
            tokio::spawn(async move {
                let _ = tx.send(TerminalEvent::Error {
                    session_id: sid,
                    error: "Terminal capability is disabled".to_string(),
                }).await;
            });
            return;
        }
```

Note: Uses `tokio::spawn` to match the existing pattern in `open()` (which is a sync fn), rather than making it async.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/terminal.rs
git commit -m "feat(agent): check CAP_TERMINAL before opening PTY"
```

---

### Task 15: Full workspace compilation

**Files:** None (verification only)

- [ ] **Step 1: Compile full workspace**

Run: `cargo build --workspace`
Expected: compiles with no errors

- [ ] **Step 2: Run existing tests**

Run: `cargo test --workspace`
Expected: all existing tests pass (no regressions)

- [ ] **Step 3: Fix any compilation or test issues**

Address any remaining errors. Common issues:
- Missing imports for new constants/types
- Signature mismatches where `capabilities` or `protocol_version` fields were added
- Serde attribute conflicts

- [ ] **Step 4: Commit any fixes**

```bash
git add crates/
git commit -m "fix: resolve compilation issues across workspace"
```

---

## Chunk 4: Frontend

### Task 16: Extract shared CAPABILITIES constant

**Files:**
- Create: `apps/web/src/lib/capabilities.ts`

- [ ] **Step 1: Create shared capabilities constant file**

Create `apps/web/src/lib/capabilities.ts`:

```typescript
export const CAP_TERMINAL = 1
export const CAP_EXEC = 2
export const CAP_UPGRADE = 4
export const CAP_PING_ICMP = 8
export const CAP_PING_TCP = 16
export const CAP_PING_HTTP = 32
export const CAP_DEFAULT = 56

export const CAPABILITIES = [
  { bit: CAP_TERMINAL, key: 'terminal', label: 'Web Terminal', risk: 'high' as const },
  { bit: CAP_EXEC, key: 'exec', label: 'Remote Exec', risk: 'high' as const },
  { bit: CAP_UPGRADE, key: 'upgrade', label: 'Auto Upgrade', risk: 'high' as const },
  { bit: CAP_PING_ICMP, key: 'ping_icmp', label: 'ICMP Ping', risk: 'low' as const },
  { bit: CAP_PING_TCP, key: 'ping_tcp', label: 'TCP Probe', risk: 'low' as const },
  { bit: CAP_PING_HTTP, key: 'ping_http', label: 'HTTP Probe', risk: 'low' as const },
] as const

export function hasCap(capabilities: number, bit: number): boolean {
  return (capabilities & bit) !== 0
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/lib/capabilities.ts
git commit -m "feat(web): add shared capabilities constants"
```

---

### Task 17: Move useServersWs to global layout + add new WS handlers

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Modify: `apps/web/src/routes/_authed.tsx`
- Modify: `apps/web/src/routes/_authed/index.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Add WS message types**

In `use-servers-ws.ts`, extend `WsMessage` union (after line 39):
```typescript
  | { type: 'capabilities_changed'; server_id: string; capabilities: number }
  | { type: 'agent_info_updated'; server_id: string; protocol_version: number }
```

- [ ] **Step 2: Add capabilities_changed handler**

In the `switch` block, add cases before `default`:
```typescript
        case 'capabilities_changed': {
          const { server_id, capabilities } = msg
          // Update WS live cache
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
            prev?.map((s) => (s.id === server_id ? { ...s, capabilities } : s))
          )
          // Update REST detail cache
          queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
            prev ? { ...prev, capabilities } : prev
          )
          // Invalidate list cache
          queryClient.invalidateQueries({ queryKey: ['servers-list'] })
          break
        }
```

- [ ] **Step 3: Add agent_info_updated handler**

```typescript
        case 'agent_info_updated': {
          const { server_id, protocol_version } = msg
          queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
            prev ? { ...prev, protocol_version } : prev
          )
          queryClient.invalidateQueries({ queryKey: ['servers-list'] })
          break
        }
```

- [ ] **Step 4: Move useServersWs() to _authed.tsx**

In `apps/web/src/routes/_authed.tsx`, import and call:
```typescript
import { useServersWs } from '@/hooks/use-servers-ws'
```

Inside `AuthedLayout()`, add `useServersWs()` call before the `return` (after the `useEffect` blocks).

- [ ] **Step 5: Remove useServersWs() from individual pages**

Remove `useServersWs()` call AND the `import { useServersWs } from '@/hooks/use-servers-ws'` from:
- `apps/web/src/routes/_authed/index.tsx`
- `apps/web/src/routes/_authed/servers/index.tsx`
- `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 6: Add capabilities route to admin-only list**

In `_authed.tsx`, add to `ADMIN_ONLY_ROUTES`:
```typescript
  '/settings/capabilities',
```

- [ ] **Step 7: Verify frontend builds**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [ ] **Step 8: Commit**

```bash
git add apps/web/src/routes/_authed.tsx apps/web/src/routes/_authed/index.tsx apps/web/src/routes/_authed/servers/index.tsx apps/web/src/routes/_authed/servers/\$id.tsx apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): move WS hook to global layout, add capabilities_changed + agent_info_updated handlers"
```

---

### Task 18: Capabilities section on server detail page

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Create CapabilitiesSection component**

Add within `$id.tsx`:

```typescript
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { CAPABILITIES } from '@/lib/capabilities'
import { useAuth } from '@/hooks/use-auth'

function CapabilitiesSection({ server }: { server: import('@/lib/api-schema').ServerResponse }) {
  const { user } = useAuth()
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: (newCaps: number) =>
      api.put(`/api/servers/${server.id}`, { capabilities: newCaps }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers', server.id] })
    },
  })

  if (user?.role !== 'admin') return null

  const caps = server.capabilities ?? 56

  const toggle = (bit: number) => {
    const newCaps = caps & bit ? caps & ~bit : caps | bit
    mutation.mutate(newCaps)
  }

  return (
    <div className="mt-6 rounded-lg border bg-card p-6">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="font-semibold">Capability Toggles</h3>
        {server.protocol_version != null && server.protocol_version < 2 && (
          <span className="rounded bg-amber-100 px-2 py-1 text-amber-600 text-xs dark:bg-amber-900/30 dark:text-amber-400">
            Agent does not support capability enforcement — upgrade recommended
          </span>
        )}
      </div>
      <div className="space-y-3">
        {CAPABILITIES.map(({ bit, label, risk }) => (
          <div key={bit} className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span>{label}</span>
              <span className={`rounded px-1.5 py-0.5 text-xs ${
                risk === 'high'
                  ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
                  : 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
              }`}>
                {risk === 'high' ? 'High Risk' : 'Low Risk'}
              </span>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={!!(caps & bit)}
              onClick={() => toggle(bit)}
              disabled={mutation.isPending}
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                caps & bit ? 'bg-primary' : 'bg-muted'
              }`}
            >
              <span className={`inline-block size-4 rounded-full bg-white transition-transform ${
                caps & bit ? 'translate-x-6' : 'translate-x-1'
              }`} />
            </button>
          </div>
        ))}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Integrate into detail page**

Add `<CapabilitiesSection server={server} />` after `<ServerEditDialog />` (line 286).

- [ ] **Step 3: Conditionally render terminal button when CAP_TERMINAL is off**

Modify the terminal button rendering (lines 188-195). The existing code already wraps it in `{isOnline && (...)}`. Change to also check capability:

Before:
```typescript
            {isOnline && (
              <Link params={{ serverId: id }} to="/terminal/$serverId">
```

After:
```typescript
            {isOnline && (server.capabilities == null || (server.capabilities & 1) !== 0) && (
              <Link params={{ serverId: id }} to="/terminal/$serverId">
```

- [ ] **Step 4: Verify frontend builds**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$id.tsx
git commit -m "feat(web): add capability toggles section to server detail page"
```

---

### Task 19: Capabilities settings page

**Files:**
- Create: `apps/web/src/routes/_authed/settings/capabilities.tsx`
- Modify: `apps/web/src/components/layout/sidebar.tsx`

- [ ] **Step 1: Create capabilities settings page**

Create `apps/web/src/routes/_authed/settings/capabilities.tsx`:

```typescript
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Search, ShieldAlert } from 'lucide-react'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import { CAPABILITIES, CAP_DEFAULT } from '@/lib/capabilities'

export const Route = createFileRoute('/_authed/settings/capabilities')({
  component: CapabilitiesPage
})

interface ServerInfo {
  id: string
  name: string
  capabilities: number
  protocol_version: number
}

function CapabilitiesPage() {
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [selected, setSelected] = useState<Set<string>>(new Set())

  const { data: servers } = useQuery<ServerInfo[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerInfo[]>('/api/servers')
  })

  const batchMutation = useMutation({
    mutationFn: (input: { server_ids: string[]; set: number; unset: number }) =>
      api.put('/api/servers/batch-capabilities', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers-list'] })
    }
  })

  const singleMutation = useMutation({
    mutationFn: ({ id, capabilities }: { id: string; capabilities: number }) =>
      api.put(`/api/servers/${id}`, { capabilities }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers-list'] })
    }
  })

  const filtered = servers?.filter((s) =>
    s.name.toLowerCase().includes(search.toLowerCase())
  ) ?? []

  const toggleAll = (checked: boolean) => {
    if (checked) {
      setSelected(new Set(filtered.map((s) => s.id)))
    } else {
      setSelected(new Set())
    }
  }

  const toggleServer = (id: string) => {
    const next = new Set(selected)
    if (next.has(id)) {
      next.delete(id)
    } else {
      next.add(id)
    }
    setSelected(next)
  }

  const toggleCap = (serverId: string, bit: number, current: number) => {
    const newCaps = current & bit ? current & ~bit : current | bit
    singleMutation.mutate({ id: serverId, capabilities: newCaps })
  }

  const batchSetCap = (bit: number) => {
    if (selected.size === 0) return
    batchMutation.mutate({ server_ids: [...selected], set: bit, unset: 0 })
  }

  const batchUnsetCap = (bit: number) => {
    if (selected.size === 0) return
    batchMutation.mutate({ server_ids: [...selected], set: 0, unset: bit })
  }

  const batchReset = () => {
    if (selected.size === 0) return
    // Reset = set default bits, unset non-default bits
    batchMutation.mutate({
      server_ids: [...selected],
      set: CAP_DEFAULT,
      unset: ~CAP_DEFAULT & 0x3f
    })
  }

  return (
    <div>
      <h1 className="mb-4 font-bold text-2xl">Capabilities</h1>

      <div className="mb-4 flex items-center gap-4">
        <div className="relative flex-1">
          <Search className="absolute top-2.5 left-3 size-4 text-muted-foreground" />
          <input
            className="w-full rounded-md border bg-background py-2 pr-4 pl-9 text-sm"
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search servers..."
            value={search}
          />
        </div>
        {selected.size > 0 && (
          <div className="flex gap-2">
            {CAPABILITIES.map((c) => (
              <div key={c.key} className="flex gap-0.5">
                <Button onClick={() => batchSetCap(c.bit)} size="sm" variant="outline">
                  +{c.label}
                </Button>
                <Button onClick={() => batchUnsetCap(c.bit)} size="sm" variant="outline">
                  -{c.label}
                </Button>
              </div>
            ))}
            <Button onClick={batchReset} size="sm" variant="outline">
              Reset
            </Button>
          </div>
        )}
      </div>

      <div className="overflow-x-auto rounded-lg border">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b bg-muted/50">
              <th className="p-3 text-left">
                <input
                  checked={filtered.length > 0 && selected.size === filtered.length}
                  onChange={(e) => toggleAll(e.target.checked)}
                  type="checkbox"
                />
              </th>
              <th className="p-3 text-left">Server</th>
              {CAPABILITIES.map((c) => (
                <th key={c.key} className="p-3 text-center">
                  {c.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {filtered.map((s) => {
              const caps = s.capabilities ?? CAP_DEFAULT
              return (
                <tr key={s.id} className="border-b last:border-0">
                  <td className="p-3">
                    <input
                      checked={selected.has(s.id)}
                      onChange={() => toggleServer(s.id)}
                      type="checkbox"
                    />
                  </td>
                  <td className="p-3">
                    <div className="flex items-center gap-2">
                      <span>{s.name}</span>
                      {s.protocol_version < 2 && (
                        <ShieldAlert className="size-4 text-amber-500" title="Agent v1 — no enforcement" />
                      )}
                    </div>
                  </td>
                  {CAPABILITIES.map((c) => (
                    <td key={c.key} className="p-3 text-center">
                      <button
                        type="button"
                        role="switch"
                        aria-checked={!!(caps & c.bit)}
                        onClick={() => toggleCap(s.id, c.bit, caps)}
                        disabled={singleMutation.isPending}
                        className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                          caps & c.bit ? 'bg-primary' : 'bg-muted'
                        }`}
                      >
                        <span className={`inline-block size-3 rounded-full bg-white transition-transform ${
                          caps & c.bit ? 'translate-x-5' : 'translate-x-1'
                        }`} />
                      </button>
                    </td>
                  ))}
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Add sidebar navigation link**

In `apps/web/src/components/layout/sidebar.tsx`, add to `navItems` array (after the "Commands" entry, around line 26):
```typescript
  { to: '/settings/capabilities', label: 'Capabilities', icon: Shield, adminOnly: true },
```

`Shield` is already imported from `lucide-react` (line 12).

- [ ] **Step 3: Verify frontend builds**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/settings/capabilities.tsx apps/web/src/components/layout/sidebar.tsx
git commit -m "feat(web): add capabilities settings page with batch management"
```

---

### Task 20: Tasks page — capability-aware UI

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/tasks.tsx`

- [ ] **Step 1: Update ServerInfo type and grey out servers without CAP_EXEC**

Update the `ServerInfo` interface (lines 13-16) to include capabilities:
```typescript
interface ServerInfo {
  id: string
  name: string
  capabilities?: number
}
```

In the server selection list, check each server's capabilities:
```typescript
const execEnabled = !s.capabilities || (s.capabilities & 2) !== 0
```

Grey out disabled servers in the rendering:
```typescript
<label className={execEnabled ? '' : 'opacity-50 cursor-not-allowed'}>
  <input
    type="checkbox"
    disabled={!execEnabled}
    // ...existing props...
  />
  {s.name}
  {!execEnabled && <span className="ml-1 text-muted-foreground text-xs">(exec disabled)</span>}
</label>
```

- [ ] **Step 2: Display synthetic results with "skipped" style**

In results rendering, check for exit_code -2:
```typescript
{result.exit_code === -2 ? (
  <div className="text-muted-foreground italic">
    Skipped — Exec is disabled for this server
  </div>
) : (
  // existing result display
)}
```

- [ ] **Step 3: Verify frontend builds**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/settings/tasks.tsx
git commit -m "feat(web): grey out exec-disabled servers and show skipped results"
```

---

### Task 21: OpenAPI + type generation + formatting

**Files:**
- Modify: `crates/server/src/openapi.rs`
- Regenerate: `apps/web/src/lib/api-types.ts`

- [ ] **Step 1: Register new schemas and endpoints in OpenAPI**

In `openapi.rs`:
- Add `crate::router::api::server::BatchCapabilitiesRequest` and `crate::router::api::server::BatchCapabilitiesResponse` to `components(schemas(...))` list
- Add `crate::router::api::server::batch_update_capabilities` to the `paths(...)` list

- [ ] **Step 2: Regenerate frontend API types**

Run: `cd apps/web && bun run generate:api-types`
Expected: `api-types.ts` updated with new fields (capabilities, protocol_version on ServerResponse, new batch endpoint types)

- [ ] **Step 3: Run Ultracite formatting**

Run: `bun x ultracite fix`
Expected: code formatted

- [ ] **Step 4: Run Clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 5: Run all tests**

Run: `cargo test --workspace && cd apps/web && bun run test`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/openapi.rs apps/web/src/lib/api-types.ts apps/web/src/lib/api-schema.ts
git commit -m "chore: update OpenAPI spec, regenerate types, lint and format"
```

---

## Chunk 5: Tests

### Task 22: Unit tests — capability helpers

**Files:**
- Modify: `crates/common/src/constants.rs` (add tests module)

- [ ] **Step 1: Write tests for has_capability**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_capability_single_bit() {
        assert!(has_capability(CAP_TERMINAL, CAP_TERMINAL));
        assert!(!has_capability(0, CAP_TERMINAL));
        assert!(!has_capability(CAP_EXEC, CAP_TERMINAL));
    }

    #[test]
    fn test_has_capability_combined() {
        let caps = CAP_TERMINAL | CAP_EXEC;
        assert!(has_capability(caps, CAP_TERMINAL));
        assert!(has_capability(caps, CAP_EXEC));
        assert!(!has_capability(caps, CAP_UPGRADE));
    }

    #[test]
    fn test_default_capabilities() {
        assert!(!has_capability(CAP_DEFAULT, CAP_TERMINAL));
        assert!(!has_capability(CAP_DEFAULT, CAP_EXEC));
        assert!(!has_capability(CAP_DEFAULT, CAP_UPGRADE));
        assert!(has_capability(CAP_DEFAULT, CAP_PING_ICMP));
        assert!(has_capability(CAP_DEFAULT, CAP_PING_TCP));
        assert!(has_capability(CAP_DEFAULT, CAP_PING_HTTP));
    }

    #[test]
    fn test_valid_mask() {
        assert_eq!(CAP_VALID_MASK, 63);
        // All defined bits are within mask
        for meta in ALL_CAPABILITIES {
            assert!(meta.bit & CAP_VALID_MASK == meta.bit);
        }
        // Bit 6+ is outside mask
        assert!(64 & !CAP_VALID_MASK != 0);
    }

    #[test]
    fn test_probe_type_to_cap() {
        assert_eq!(probe_type_to_cap("icmp"), Some(CAP_PING_ICMP));
        assert_eq!(probe_type_to_cap("tcp"), Some(CAP_PING_TCP));
        assert_eq!(probe_type_to_cap("http"), Some(CAP_PING_HTTP));
        assert_eq!(probe_type_to_cap("unknown"), None);
    }

    #[test]
    fn test_u32_max_allows_everything() {
        // u32::MAX is used as initial value before Welcome arrives
        assert!(has_capability(u32::MAX, CAP_TERMINAL));
        assert!(has_capability(u32::MAX, CAP_EXEC));
        assert!(has_capability(u32::MAX, CAP_UPGRADE));
        assert!(has_capability(u32::MAX, CAP_PING_ICMP));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p serverbee-common`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/common/src/constants.rs
git commit -m "test(common): add unit tests for capability helpers"
```

---

### Task 23: Unit tests — protocol serialization

**Files:**
- Modify: `crates/common/src/protocol.rs` (add tests module)

- [ ] **Step 1: Write protocol serialization tests**

Note: `AgentMessage::SystemInfo` uses `#[serde(flatten)]` for `info`, so SystemInfo fields are at the top level (not nested under `info`).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welcome_without_capabilities_deserializes() {
        // Old Server sends Welcome without capabilities field
        let json = r#"{"type":"welcome","server_id":"s1","protocol_version":1,"report_interval":3}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Welcome { capabilities, .. } => {
                assert_eq!(capabilities, None);
            }
            _ => panic!("Expected Welcome"),
        }
    }

    #[test]
    fn test_welcome_with_capabilities_deserializes() {
        let json = r#"{"type":"welcome","server_id":"s1","protocol_version":2,"report_interval":3,"capabilities":56}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Welcome { capabilities, .. } => {
                assert_eq!(capabilities, Some(56));
            }
            _ => panic!("Expected Welcome"),
        }
    }

    #[test]
    fn test_capabilities_sync_round_trip() {
        let msg = ServerMessage::CapabilitiesSync { capabilities: 7 };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::CapabilitiesSync { capabilities } => {
                assert_eq!(capabilities, 7);
            }
            _ => panic!("Expected CapabilitiesSync"),
        }
    }

    #[test]
    fn test_capability_denied_round_trip() {
        let msg = AgentMessage::CapabilityDenied {
            msg_id: Some("task-1".to_string()),
            session_id: None,
            capability: "exec".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::CapabilityDenied { msg_id, session_id, capability } => {
                assert_eq!(msg_id, Some("task-1".to_string()));
                assert_eq!(session_id, None);
                assert_eq!(capability, "exec");
            }
            _ => panic!("Expected CapabilityDenied"),
        }
    }

    #[test]
    fn test_system_info_without_protocol_version() {
        // Old Agent sends SystemInfo without protocol_version — fields are flattened
        let json = r#"{"type":"system_info","msg_id":"m1","cpu_name":"Intel","cpu_cores":4,"cpu_arch":"x86_64","os":"Linux","kernel_version":"5.4","mem_total":8000000000,"swap_total":0,"disk_total":100000000000,"agent_version":"0.1.0"}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::SystemInfo { info, .. } => {
                assert_eq!(info.protocol_version, 1); // default
            }
            _ => panic!("Expected SystemInfo"),
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p serverbee-common`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "test(common): add protocol serialization tests for capability messages"
```

---

### Task 24: Frontend tests

**Files:**
- Create: `apps/web/src/lib/capabilities.test.ts`

- [ ] **Step 1: Test capability bit operations**

```typescript
import { describe, expect, it } from 'vitest'
import { CAP_DEFAULT, CAP_EXEC, CAP_PING_HTTP, CAP_PING_ICMP, CAP_PING_TCP, CAP_TERMINAL, hasCap } from './capabilities'

describe('capability toggles', () => {
  it('default capabilities have ping enabled, terminal disabled', () => {
    expect(hasCap(CAP_DEFAULT, CAP_TERMINAL)).toBe(false)
    expect(hasCap(CAP_DEFAULT, CAP_EXEC)).toBe(false)
    expect(hasCap(CAP_DEFAULT, CAP_PING_ICMP)).toBe(true)
    expect(hasCap(CAP_DEFAULT, CAP_PING_TCP)).toBe(true)
    expect(hasCap(CAP_DEFAULT, CAP_PING_HTTP)).toBe(true)
  })

  it('toggle on adds bit', () => {
    const caps = CAP_DEFAULT
    const newCaps = caps | CAP_TERMINAL
    expect(hasCap(newCaps, CAP_TERMINAL)).toBe(true)
  })

  it('toggle off removes bit', () => {
    const caps = CAP_DEFAULT | CAP_TERMINAL
    const newCaps = caps & ~CAP_TERMINAL
    expect(hasCap(newCaps, CAP_TERMINAL)).toBe(false)
  })
})
```

- [ ] **Step 2: Run tests**

Run: `cd apps/web && bun run test`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/capabilities.test.ts
git commit -m "test(web): add capability toggle unit tests"
```

---

### Task 25: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`

- [ ] **Step 2: Full test suite**

Run: `cargo test --workspace`

- [ ] **Step 3: Frontend build**

Run: `cd apps/web && bun run build`

- [ ] **Step 4: Frontend tests**

Run: `cd apps/web && bun run test`

- [ ] **Step 5: Lint**

Run: `cargo clippy --workspace -- -D warnings && cd apps/web && bun x ultracite check`

- [ ] **Step 6: Final commit if needed**

```bash
git add crates/ apps/web/
git commit -m "chore: final verification — all tests pass, lint clean"
```
