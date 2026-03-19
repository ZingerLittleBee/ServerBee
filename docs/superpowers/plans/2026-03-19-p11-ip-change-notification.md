# P11: IP Change Notification Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect and notify when a server's IP address changes, via both server-side passive detection (on Agent connect) and agent-side active detection (periodic NIC enumeration + optional external IP API).

**Architecture:** Add `last_remote_addr` to servers table. Server compares on WS connect. Agent adds 5-min IP check timer with `IpChanged` protocol message. Alert system extended with event-driven `ip_changed` rule type. `AlertStateManager` promoted from local variable to `AppState` for shared access.

**Tech Stack:** Rust (Axum, sea-orm, sysinfo, tokio), React (TanStack Router/Query)

**Spec:** `docs/superpowers/specs/2026-03-19-batch1-batch2-features-design.md` Section 3

---

## File Structure

### Modified Files (Common)
- `crates/common/src/protocol.rs` — Add `IpChanged` variant to `AgentMessage`, `ServerIpChanged` to `BrowserMessage`
- `crates/common/src/types.rs` — Add `NetworkInterface` struct

### Modified Files (Agent)
- `crates/agent/src/config.rs` — Add `IpChangeConfig` struct
- `crates/agent/src/reporter.rs` — Add 5-min IP detection timer + `IpChanged` sending

### Modified Files (Server)
- `crates/server/src/migration/m20260319_000007_service_monitor.rs` — Add `ALTER TABLE servers ADD COLUMN last_remote_addr`
- `crates/server/src/entity/server.rs` — Add `last_remote_addr` field
- `crates/server/src/router/ws/agent.rs` — Handle `IpChanged` message + passive detection on connect
- `crates/server/src/service/alert.rs` — Add `check_event_rules()` method + `ip_changed` rule type
- `crates/server/src/state.rs` — Add `alert_state_manager: AlertStateManager` field
- `crates/server/src/task/alert_evaluator.rs` — Use `state.alert_state_manager` instead of local
- `crates/server/src/openapi.rs` — Update schemas

### Modified Files (Frontend)
- `apps/web/src/routes/_authed/settings/alerts.tsx` — Add "IP Changed" to rule_type dropdown

---

### Task 1: Protocol Extension

**Files:**
- Modify: `crates/common/src/types.rs`
- Modify: `crates/common/src/protocol.rs`

- [ ] **Step 1: Add `NetworkInterface` type**

In `crates/common/src/types.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
}
```

- [ ] **Step 2: Add `IpChanged` to AgentMessage**

In `crates/common/src/protocol.rs`, add to `AgentMessage` enum:

```rust
IpChanged {
    ipv4: Option<String>,
    ipv6: Option<String>,
    interfaces: Vec<NetworkInterface>,
},
```

- [ ] **Step 3: Add `ServerIpChanged` to BrowserMessage**

```rust
ServerIpChanged {
    server_id: String,
    old_ipv4: Option<String>,
    new_ipv4: Option<String>,
    old_ipv6: Option<String>,
    new_ipv6: Option<String>,
    old_remote_addr: Option<String>,
    new_remote_addr: Option<String>,
},
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check --workspace`

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/
git commit -m "feat(common): add IpChanged protocol messages and NetworkInterface type"
```

---

### Task 2: Migration — Add `last_remote_addr` Column

**Files:**
- Modify: `crates/server/src/migration/m20260319_000007_service_monitor.rs`
- Modify: `crates/server/src/entity/server.rs`

- [ ] **Step 1: Add ALTER TABLE to migration**

At the end of the `up()` method in the service monitor migration, add:

```rust
// Add last_remote_addr to servers table
manager
    .alter_table(
        Table::alter()
            .table(Alias::new("servers"))
            .add_column(ColumnDef::new(Alias::new("last_remote_addr")).text().null())
            .to_owned(),
    )
    .await?;
```

- [ ] **Step 2: Add field to server entity**

In `crates/server/src/entity/server.rs`, add:
```rust
pub last_remote_addr: Option<String>,
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/ crates/server/src/entity/server.rs
git commit -m "feat(server): add last_remote_addr column to servers table"
```

---

### Task 3: Promote AlertStateManager to AppState

**Files:**
- Modify: `crates/server/src/state.rs`
- Modify: `crates/server/src/task/alert_evaluator.rs`

- [ ] **Step 1: Add AlertStateManager to AppState**

In `crates/server/src/state.rs`, add:
```rust
use crate::service::alert::AlertStateManager;

pub struct AppState {
    // ...existing fields...
    pub alert_state_manager: AlertStateManager,
}
```

In `AppState::new()`, initialize it:
```rust
let alert_state_manager = AlertStateManager::load_from_db(&db).await
    .unwrap_or_else(|e| {
        tracing::warn!("Failed to load alert states: {e}, starting fresh");
        AlertStateManager::new()
    });
```

- [ ] **Step 2: Update alert_evaluator to use shared state**

In `crates/server/src/task/alert_evaluator.rs`, replace the local `AlertStateManager` creation with `&state.alert_state_manager`:

```rust
pub async fn run(state: Arc<AppState>) {
    tracing::info!("Alert evaluator started");
    let state_manager = &state.alert_state_manager;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Err(e) = AlertService::evaluate_all(&state.db, &state.agent_manager, state_manager).await {
            tracing::error!("Alert evaluation error: {e}");
        }
    }
}
```

- [ ] **Step 3: Ensure AlertStateManager has a `new()` constructor**

Check `crates/server/src/service/alert.rs` — if `AlertStateManager::new()` doesn't exist (only `load_from_db`), add it as an empty constructor.

- [ ] **Step 4: Run existing alert tests to verify no regression**

Run: `cargo test -p serverbee-server -- alert`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/state.rs crates/server/src/task/alert_evaluator.rs crates/server/src/service/alert.rs
git commit -m "refactor(server): promote AlertStateManager to AppState for shared access"
```

---

### Task 4: Event-Driven Alert Rules

**Files:**
- Modify: `crates/server/src/service/alert.rs`

- [ ] **Step 1: Add `check_event_rules()` method**

```rust
impl AlertService {
    pub async fn check_event_rules(
        db: &DatabaseConnection,
        state_manager: &AlertStateManager,
        server_id: &str,
        event_type: &str,
    ) -> Result<(), AppError> {
        // 1. Load enabled rules
        let rules = alert_rule::Entity::find()
            .filter(alert_rule::Column::Enabled.eq(true))
            .all(db).await?;

        // 2. Filter rules that contain an item with matching rule_type
        for rule in &rules {
            let items: Vec<AlertRuleItem> = serde_json::from_str(&rule.rules_json).unwrap_or_default();
            if !items.iter().any(|item| item.rule_type == event_type) {
                continue;
            }
            // 3. Check cover_type/server_ids
            if !Self::covers_server(rule, server_id) {
                continue;
            }
            // 4. Use state_manager for dedup (once/always)
            // 5. Fire notification
            if let Some(ref ngid) = rule.notification_group_id {
                let title = format!("IP Changed: {server_id}");
                let msg = format!("Server {server_id} IP address has changed");
                NotificationService::send_group(db, ngid, &title, &msg).await.ok();
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 2: Ensure evaluate_all skips event-driven rules**

In the existing `evaluate_all()` loop, add a check to skip rules where all items have `rule_type == "ip_changed"` (or other event types).

- [ ] **Step 3: Write unit test**

Test that `check_event_rules` correctly matches rules and fires for the right server.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/alert.rs
git commit -m "feat(server): add event-driven alert rule dispatch for ip_changed"
```

---

### Task 5: Server-Side Passive Detection

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Add IP comparison on Agent connect**

In the SystemInfo handling section of `agent.rs`, after updating the server record, add:

```rust
// Passive IP change detection
let remote_ip = remote_addr.ip().to_string();
if let Some(ref last_addr) = server_model.last_remote_addr {
    if last_addr != &remote_ip {
        tracing::info!("Server {server_id} remote_addr changed: {last_addr} -> {remote_ip}");
        // Update DB
        // Write audit log
        // Fire event-driven alert
        AlertService::check_event_rules(&state.db, &state.alert_state_manager, &server_id, "ip_changed").await.ok();
        // Broadcast to browsers
        state.agent_manager.broadcast_browser(BrowserMessage::ServerIpChanged { ... });
    }
}
// Always update last_remote_addr
// UPDATE servers SET last_remote_addr = ? WHERE id = ?
```

- [ ] **Step 2: Handle `IpChanged` agent message**

In the main message dispatch match of `agent.rs`, add:

```rust
AgentMessage::IpChanged { ipv4, ipv6, interfaces } => {
    // Update servers.ipv4 / ipv6 if changed
    // Re-run GeoIP if available
    // Write audit log
    // Fire event-driven alert
    // Broadcast to browsers
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/ws/agent.rs
git commit -m "feat(server): add passive IP change detection and IpChanged message handler"
```

---

### Task 6: Agent-Side Active Detection

**Files:**
- Modify: `crates/agent/src/config.rs`
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Add IpChangeConfig to agent config**

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IpChangeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub check_external_ip: bool,
    #[serde(default = "default_external_ip_url")]
    pub external_ip_url: String,
    #[serde(default = "default_ip_interval")]
    pub interval_secs: u64,
}

fn default_external_ip_url() -> String { "https://api.ipify.org".to_string() }
fn default_ip_interval() -> u64 { 300 }

impl Default for IpChangeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_external_ip: false,
            external_ip_url: default_external_ip_url(),
            interval_secs: 300,
        }
    }
}
```

Add `#[serde(default)] pub ip_change: IpChangeConfig,` to `AgentConfig`.

- [ ] **Step 2: Add IP detection timer to reporter**

In the main `tokio::select!` loop in `reporter.rs`, add a new arm:

```rust
// IP change detection
_ = ip_check_interval.tick(), if config.ip_change.enabled => {
    let new_ips = collect_interface_ips();
    if new_ips != cached_ips {
        let msg = AgentMessage::IpChanged { ... };
        send_message(&mut write, &msg).await;
        cached_ips = new_ips;
    }
}
```

`collect_interface_ips()` uses `sysinfo::Networks` to enumerate all interface IPs. If `check_external_ip` is true, also query the external IP URL.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-agent`

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/
git commit -m "feat(agent): add periodic IP change detection with IpChanged message"
```

---

### Task 7: Frontend — Alert Rule Type

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/alerts.tsx`

- [ ] **Step 1: Add "IP Changed" to rule_type options**

In the alert rule form's `rule_type` select/dropdown, add `{ value: "ip_changed", label: "IP Changed" }`. When selected, hide the threshold fields (min/max/duration) since this is event-driven.

- [ ] **Step 2: Verify frontend**

Run: `cd apps/web && bun run typecheck && bun x ultracite check`

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/settings/alerts.tsx
git commit -m "feat(web): add IP Changed rule type to alert management UI"
```

---

### Task 8: Final Verification

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace`

- [ ] **Step 2: Run clippy + lint**

Run: `cargo clippy --workspace -- -D warnings && cd apps/web && bun x ultracite check`

- [ ] **Step 3: Update docs**

Update TESTING.md, PROGRESS.md, and ENV.md with P11 completion and new agent config vars.

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "docs: update TESTING.md, PROGRESS.md, and ENV.md for P11 IP change notification"
```
