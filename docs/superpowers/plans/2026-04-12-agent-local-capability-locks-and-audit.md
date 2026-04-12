# Agent Local Capability Locks And High-Risk Audit Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add agent-side local capability locks that the server cannot override, surface the resulting effective capabilities in the API/UI, and expand high-risk user-operation audit coverage with structured JSON details.

**Architecture:** Keep the existing server capability bitmap as the control-plane policy, add an agent-local bitmap computed from repeatable CLI flags, and derive `effective_caps = server_caps & agent_local_caps` at both ends. The server stores agent-local state in memory, returns both runtime and configured capability views to the web app, and records user-initiated high-risk reads/sessions in the existing `audit_logs` table with structured JSON detail.

**Tech Stack:** Rust (Axum, tokio, DashMap, sea-orm, serde), React (TanStack Query/Router, shadcn/ui, Vitest), OpenAPI generation (`dump_openapi`, `openapi-typescript`)

**Spec:** `docs/superpowers/specs/2026-04-12-agent-local-capability-locks-and-audit-design.md`

---

## File Map

### Common + Agent

- Modify: `crates/common/src/constants.rs`
- Modify: `crates/common/src/protocol.rs`
- Create: `crates/agent/src/capability_policy.rs`
- Modify: `crates/agent/src/main.rs`
- Modify: `crates/agent/src/reporter.rs`
- Modify: `crates/agent/src/terminal.rs` if terminal error propagation needs capability-specific metadata

### Server Runtime + API

- Modify: `crates/server/src/service/agent_manager.rs`
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/router/ws/terminal.rs`
- Modify: `crates/server/src/router/ws/docker_logs.rs`
- Modify: `crates/server/src/router/api/file.rs`
- Modify: `crates/server/src/router/api/docker.rs`
- Modify: `crates/server/src/router/api/task.rs`
- Modify: `crates/server/src/task/task_scheduler.rs`

### Audit Support

- Create: `crates/server/src/service/high_risk_audit.rs`
- Modify: `crates/server/src/service/mod.rs`
- Modify: `crates/server/src/state.rs`
- Modify: `crates/server/src/service/audit.rs` only if a small helper for JSON detail reduces handler duplication

### Frontend + Generated Types

- Modify: `apps/web/src/lib/capabilities.ts`
- Modify: `apps/web/src/lib/capabilities.test.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.test.ts`
- Modify: `apps/web/src/routes/_authed/settings/capabilities.tsx`
- Modify: `apps/web/src/components/server/capabilities-dialog.tsx`
- Modify: `apps/web/src/components/server/capabilities-dialog.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$serverId/docker/index.tsx`
- Modify: `apps/web/src/routes/_authed/settings/tasks.tsx`
- Modify: `apps/web/src/components/task/scheduled-task-dialog.tsx`
- Modify: `apps/web/openapi.json`
- Modify: `apps/web/src/lib/api-types.ts`

### Docs

- Modify: `apps/docs/content/docs/en/agent.mdx`
- Modify: `apps/docs/content/docs/cn/agent.mdx`
- Modify: `apps/docs/content/docs/en/capabilities.mdx`
- Modify: `apps/docs/content/docs/cn/capabilities.mdx`

### Tests

- Modify: `crates/server/tests/integration.rs`
- Modify: `crates/server/tests/docker_integration.rs`

---

## Chunk 1: Shared Types And Agent Capability Policy

### Task 1: Add shared capability key/reason types and extend protocol messages

**Files:**
- Modify: `crates/common/src/constants.rs`
- Modify: `crates/common/src/protocol.rs`

- [ ] **Step 1: Add shared enums and helpers to the common crate**

In `crates/common/src/constants.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityKey {
    Terminal,
    Exec,
    Upgrade,
    PingIcmp,
    PingTcp,
    PingHttp,
    File,
    Docker,
}

impl CapabilityKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Exec => "exec",
            Self::Upgrade => "upgrade",
            Self::PingIcmp => "ping_icmp",
            Self::PingTcp => "ping_tcp",
            Self::PingHttp => "ping_http",
            Self::File => "file",
            Self::Docker => "docker",
        }
    }

    pub fn to_bit(self) -> u32 {
        match self {
            Self::Terminal => CAP_TERMINAL,
            Self::Exec => CAP_EXEC,
            Self::Upgrade => CAP_UPGRADE,
            Self::PingIcmp => CAP_PING_ICMP,
            Self::PingTcp => CAP_PING_TCP,
            Self::PingHttp => CAP_PING_HTTP,
            Self::File => CAP_FILE,
            Self::Docker => CAP_DOCKER,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDeniedReason {
    ServerCapabilityDisabled,
    AgentCapabilityDisabled,
}
```

Also add a helper:

```rust
pub fn effective_capabilities(server_caps: u32, agent_local_caps: u32) -> u32 {
    server_caps & agent_local_caps
}
```

- [ ] **Step 2: Add `FromStr` tests before implementation**

At the bottom of `crates/common/src/constants.rs`, add tests that assert:

- `"terminal".parse::<CapabilityKey>()` succeeds
- `"ping_http".parse::<CapabilityKey>()` succeeds
- unknown strings fail
- `effective_capabilities(CAP_EXEC | CAP_FILE, CAP_FILE) == CAP_FILE`

Run: `cargo test -p serverbee-common constants`
Expected: FAIL until `FromStr` is implemented

- [ ] **Step 3: Implement `FromStr` for `CapabilityKey`**

Use `impl std::str::FromStr for CapabilityKey` returning a small error string:

```rust
match value {
    "terminal" => Ok(Self::Terminal),
    "exec" => Ok(Self::Exec),
    "upgrade" => Ok(Self::Upgrade),
    "ping_icmp" => Ok(Self::PingIcmp),
    "ping_tcp" => Ok(Self::PingTcp),
    "ping_http" => Ok(Self::PingHttp),
    "file" => Ok(Self::File),
    "docker" => Ok(Self::Docker),
    _ => Err(format!("unknown capability: {value}")),
}
```

- [ ] **Step 4: Extend `AgentMessage::SystemInfo` and `CapabilityDenied`**

In `crates/common/src/protocol.rs`, change:

```rust
AgentMessage::SystemInfo {
    msg_id: String,
    #[serde(flatten)]
    info: SystemInfo,
}
```

to:

```rust
AgentMessage::SystemInfo {
    msg_id: String,
    #[serde(flatten)]
    info: SystemInfo,
    #[serde(default)]
    agent_local_capabilities: Option<u32>,
}
```

Change `CapabilityDenied` to:

```rust
CapabilityDenied {
    msg_id: Option<String>,
    session_id: Option<String>,
    capability: String,
    reason: crate::constants::CapabilityDeniedReason,
}
```

- [ ] **Step 5: Extend browser capability updates**

In `BrowserMessage::CapabilitiesChanged`, replace the single capability field with:

```rust
CapabilitiesChanged {
    server_id: String,
    capabilities: u32,
    agent_local_capabilities: Option<u32>,
    effective_capabilities: Option<u32>,
}
```

Add round-trip tests in `crates/common/src/protocol.rs` for the new fields.

- [ ] **Step 6: Verify the shared crate**

Run: `cargo test -p serverbee-common`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/common/src/constants.rs crates/common/src/protocol.rs
git commit -m "feat(common): add local capability policy types and protocol fields"
```

---

### Task 2: Add agent-side CLI parsing and local capability policy calculation

**Files:**
- Create: `crates/agent/src/capability_policy.rs`
- Modify: `crates/agent/src/main.rs`

- [ ] **Step 1: Create failing tests for CLI parsing and default policy**

Create `crates/agent/src/capability_policy.rs` with a `#[cfg(test)]` module first. Add tests for:

- no flags => `CAP_DEFAULT`
- `--allow-cap exec` adds `CAP_EXEC`
- `--deny-cap ping_http` removes `CAP_PING_HTTP`
- `--allow-cap exec --deny-cap exec` leaves `CAP_EXEC` off
- duplicate flags do not panic
- unknown capability returns an error

Target API:

```rust
pub struct CapabilityCliOverrides {
    pub allow_caps: Vec<CapabilityKey>,
    pub deny_caps: Vec<CapabilityKey>,
}

pub fn parse_capability_args<I>(args: I) -> anyhow::Result<CapabilityCliOverrides>
where
    I: IntoIterator<Item = String>;

pub fn compute_agent_local_capabilities(overrides: &CapabilityCliOverrides) -> u32;
```

Run: `cargo test -p serverbee-agent capability_policy`
Expected: FAIL with missing module/functions

- [ ] **Step 2: Implement the parser with `std::env::args()` semantics**

Implement a small positional parser that only consumes:

- `--allow-cap <value>`
- `--deny-cap <value>`

Reject:

- missing values
- unknown flags that look like capability flags
- unknown capability names

Ignore unrelated argv segments so the binary keeps room for future CLI additions.

- [ ] **Step 3: Wire the module into `main.rs`**

Add `mod capability_policy;` and compute overrides before the reporter is constructed:

```rust
let cli_overrides = capability_policy::parse_capability_args(std::env::args())?;
let agent_local_capabilities =
    capability_policy::compute_agent_local_capabilities(&cli_overrides);
```

Pass `agent_local_capabilities` into `Reporter::new(...)`.

- [ ] **Step 4: Update `Reporter::new` signature without disturbing registration flow**

Change:

```rust
pub fn new(config: AgentConfig, fingerprint: String) -> Self
```

to:

```rust
pub fn new(config: AgentConfig, fingerprint: String, agent_local_capabilities: u32) -> Self
```

Keep token registration and config loading untouched.

- [ ] **Step 5: Verify the agent CLI module**

Run: `cargo test -p serverbee-agent capability_policy`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent/src/capability_policy.rs crates/agent/src/main.rs
git commit -m "feat(agent): add local capability CLI policy parser"
```

---

### Task 3: Compute effective capabilities on the agent and send them to the server

**Files:**
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Add reporter tests for effective capability calculation**

Add focused tests in `crates/agent/src/reporter.rs` for:

- welcome `capabilities: Some(CAP_EXEC | CAP_FILE)` with local `CAP_FILE` => effective `CAP_FILE`
- welcome `capabilities: None` with local `CAP_DEFAULT` => effective `CAP_DEFAULT`
- `CapabilitiesSync` recomputes effective caps instead of overwriting the local policy

Run: `cargo test -p serverbee-agent reporter`
Expected: FAIL until logic is updated

- [ ] **Step 2: Store local policy and compute effective caps in one place**

Keep a fixed `agent_local_capabilities: u32` on `Reporter`, and replace the current `AtomicU32::new(u32::MAX)` bootstrapping logic with:

```rust
let effective_caps = Arc::new(AtomicU32::new(agent_local_capabilities));
```

On `Welcome` and `CapabilitiesSync`:

```rust
let server_caps = caps.unwrap_or(u32::MAX);
let effective = effective_capabilities(server_caps, self.agent_local_capabilities);
effective_caps.store(effective, Ordering::SeqCst);
```

- [ ] **Step 3: Send `agent_local_capabilities` in `AgentMessage::SystemInfo`**

Change the initial message construction in `reporter.rs` to:

```rust
let info_msg = AgentMessage::SystemInfo {
    msg_id: uuid::Uuid::new_v4().to_string(),
    info: serverbee_common::types::SystemInfo { ... },
    agent_local_capabilities: Some(self.agent_local_capabilities),
};
```

- [ ] **Step 4: Add structured capability deny reasons where the protocol already supports them**

For `Exec`, `Upgrade`, and `Traceroute`, update `AgentMessage::CapabilityDenied` construction to set:

```rust
reason: CapabilityDeniedReason::AgentCapabilityDisabled,
```

Leave terminal/file/docker read paths to be explained by server-side effective-capability checks rather than inventing ad hoc agent messages.

- [ ] **Step 5: Verify agent behavior**

Run: `cargo test -p serverbee-agent reporter`
Expected: PASS

Run: `cargo check -p serverbee-agent`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): apply local capability policy and report runtime bitmap"
```

---

## Chunk 2: Server Runtime Capability Model And Response Surface

### Task 4: Track configured, local, and effective capabilities in `AgentManager`

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs`
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Add failing tests for runtime capability snapshots**

In `crates/server/src/service/agent_manager.rs`, add tests for:

- server caps present + local caps present => effective intersection
- server caps present + local caps absent => effective is `None`
- removing a connection clears agent-local capability state

Target helper API:

```rust
pub fn update_server_capabilities(&self, server_id: &str, caps: u32);
pub fn update_agent_local_capabilities(&self, server_id: &str, caps: u32);
pub fn get_agent_local_capabilities(&self, server_id: &str) -> Option<u32>;
pub fn get_effective_capabilities(&self, server_id: &str) -> Option<u32>;
```

Run: `cargo test -p serverbee-server test_protocol_version -- --nocapture`
Expected: FAIL after adding the new tests until the helpers exist

- [ ] **Step 2: Split the current capability storage into configured and local state**

In `AgentManager`, rename the existing `capabilities` map to `server_capabilities` and add:

```rust
agent_local_capabilities: DashMap<String, u32>,
```

Add the helper methods from the test target and make `remove_connection()` clear both maps.

- [ ] **Step 3: Ingest `agent_local_capabilities` from `AgentMessage::SystemInfo`**

In `crates/server/src/router/ws/agent.rs`, update:

```rust
AgentMessage::SystemInfo { msg_id, info } => { ... }
```

to:

```rust
AgentMessage::SystemInfo {
    msg_id,
    info,
    agent_local_capabilities,
} => { ... }
```

If the field is `Some(bits)`, call `state.agent_manager.update_agent_local_capabilities(server_id, bits)`.

- [ ] **Step 4: Broadcast the full runtime capability view after the first `SystemInfo`**

When `agent_local_capabilities` is ingested, compute:

```rust
let configured = server.capabilities as u32;
let effective = effective_capabilities(configured, bits);
```

Then broadcast:

```rust
BrowserMessage::CapabilitiesChanged {
    server_id: server_id.to_string(),
    capabilities: configured,
    agent_local_capabilities: Some(bits),
    effective_capabilities: Some(effective),
}
```

- [ ] **Step 5: Verify runtime map behavior**

Run: `cargo test -p serverbee-server test_protocol_version -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/agent_manager.rs crates/server/src/router/ws/agent.rs
git commit -m "feat(server): track configured and agent-local capability bitmaps"
```

---

### Task 5: Return runtime capability fields from server APIs and use effective caps at server gates

**Files:**
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/router/api/file.rs`
- Modify: `crates/server/src/router/api/docker.rs`
- Modify: `crates/server/src/router/ws/terminal.rs`
- Modify: `crates/server/src/router/ws/docker_logs.rs`
- Modify: `crates/server/src/router/api/task.rs`
- Modify: `crates/server/src/task/task_scheduler.rs`

- [ ] **Step 1: Add failing integration coverage for the new response fields**

Extend `crates/server/tests/integration.rs` with a test that:

1. registers an agent
2. sends `SystemInfo` with `agent_local_capabilities`
3. fetches `/api/servers/{id}`
4. asserts `capabilities`, `agent_local_capabilities`, and `effective_capabilities` are all present and correct

Run: `cargo test -p serverbee-server --test integration capability`
Expected: FAIL until the API shape changes

- [ ] **Step 2: Enrich `ServerResponse` with runtime fields**

In `crates/server/src/router/api/server.rs`, add:

```rust
pub agent_local_capabilities: Option<i32>,
pub effective_capabilities: Option<i32>,
```

Replace the bare `impl From<server::Model> for ServerResponse` usage with a helper that takes `&AppState` (or `&AgentManager`) so `list_servers`, `get_server`, and `update_server` can inject runtime fields from memory.

- [ ] **Step 3: Broadcast full capability state on updates**

In both `update_server()` and `batch_update_capabilities()`, replace the old broadcast with:

```rust
let agent_local = state.agent_manager.get_agent_local_capabilities(server_id);
let effective = agent_local.map(|bits| effective_capabilities(new_caps, bits));

BrowserMessage::CapabilitiesChanged {
    server_id: server_id.clone(),
    capabilities: new_caps,
    agent_local_capabilities: agent_local,
    effective_capabilities: effective,
}
```

- [ ] **Step 4: Add a shared helper for capability-denied reasons at server entry points**

In `server.rs` or a small helper inside the API modules, add logic that distinguishes:

```rust
if !has_capability(configured_caps, bit) {
    return Err(AppError::Forbidden("server_capability_disabled".into()));
}
if let Some(local) = state.agent_manager.get_agent_local_capabilities(server_id)
    && !has_capability(local, bit)
{
    return Err(AppError::Forbidden("agent_capability_disabled".into()));
}
```

Use this pattern in:

- file read/download gates
- docker REST read gates
- terminal WS gate
- docker logs WS gate
- task/exec dispatch filtering

- [ ] **Step 5: Update exec-denied persistence to preserve the new reason**

When `task.rs` or `task_scheduler.rs` skips a server because exec is disabled, use the specific reason string in the task result output, for example:

```rust
"Capability denied: exec blocked by agent local policy"
```

or:

```rust
"Capability denied: exec disabled on server"
```

Keep the path user-facing and debuggable.

- [ ] **Step 6: Verify the server routes**

Run: `cargo test -p serverbee-server --test integration capability`
Expected: PASS

Run: `cargo test -p serverbee-server --test docker_integration capability`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/router/api/server.rs crates/server/src/router/api/file.rs crates/server/src/router/api/docker.rs crates/server/src/router/ws/terminal.rs crates/server/src/router/ws/docker_logs.rs crates/server/src/router/api/task.rs crates/server/src/task/task_scheduler.rs
git commit -m "feat(server): expose runtime capability state and gate with effective caps"
```

---

## Chunk 3: Structured High-Risk Audit Expansion

### Task 6: Add audit context types and shared helpers for high-risk operations

**Files:**
- Create: `crates/server/src/service/high_risk_audit.rs`
- Modify: `crates/server/src/service/mod.rs`
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Add failing unit tests for JSON detail serialization**

Create `crates/server/src/service/high_risk_audit.rs` with tests first. Cover:

- `TerminalAuditContext` serializes `server_id`, `user_id`, `ip`, timestamps
- `DockerLogsAuditContext` serializes `container_id`, `follow`, `tail`
- `DockerViewResource` serializes stable string values (`containers`, `events`, etc.)

Run: `cargo test -p serverbee-server high_risk_audit`
Expected: FAIL until the module exists

- [ ] **Step 2: Create focused context structs**

Add:

```rust
pub struct TerminalAuditContext {
    pub server_id: String,
    pub user_id: String,
    pub ip: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

pub struct DockerLogsAuditContext {
    pub server_id: String,
    pub user_id: String,
    pub ip: String,
    pub container_id: String,
    pub tail: Option<u64>,
    pub follow: bool,
    pub started_at: chrono::DateTime<chrono::Utc>,
}
```

Also add a small `DockerViewResource` enum with an `as_str()` helper.

- [ ] **Step 3: Register the module and state maps**

In `crates/server/src/service/mod.rs`:

```rust
pub mod high_risk_audit;
```

In `crates/server/src/state.rs`, add:

```rust
pub terminal_audit_contexts: DashMap<String, TerminalAuditContext>,
pub docker_logs_audit_contexts: DashMap<String, DockerLogsAuditContext>,
```

Initialize them in `AppState::new`.

- [ ] **Step 4: Verify the helper module**

Run: `cargo test -p serverbee-server high_risk_audit`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/high_risk_audit.rs crates/server/src/service/mod.rs crates/server/src/state.rs
git commit -m "feat(server): add high-risk audit context helpers"
```

---

### Task 7: Instrument terminal, file, docker, and exec actions with structured audit logs

**Files:**
- Modify: `crates/server/src/router/ws/terminal.rs`
- Modify: `crates/server/src/router/ws/docker_logs.rs`
- Modify: `crates/server/src/router/api/file.rs`
- Modify: `crates/server/src/router/api/docker.rs`
- Modify: `crates/server/src/router/api/task.rs`
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Add failing integration tests for the new audit actions**

Extend `crates/server/tests/integration.rs` with targeted tests that assert audit entries appear for:

- `terminal_opened` / `terminal_open_denied` / `terminal_closed`
- `file_read` / `file_read_denied`
- `file_download` / `file_download_denied`
- `exec_started` / `exec_denied`

Extend `crates/server/tests/docker_integration.rs` for:

- `docker_view`
- `docker_view_denied`
- `docker_logs_subscribed`
- `docker_logs_unsubscribed`

Run: `cargo test -p serverbee-server --test integration audit`
Expected: FAIL until handlers are instrumented

- [ ] **Step 2: Add terminal session lifecycle audit logging**

In `crates/server/src/router/ws/terminal.rs`:

- capture `user_id` and `ip` before upgrade
- on successful session creation, insert a `TerminalAuditContext` into `state.terminal_audit_contexts`
- write `terminal_opened`
- on refusal due to role or capability, write `terminal_open_denied`
- on loop exit, remove the context and write `terminal_closed` with `duration_ms` and `close_reason`

Use JSON detail, for example:

```rust
serde_json::json!({
    "server_id": server_id,
    "session_id": session_id,
    "started_at": started_at,
    "ended_at": ended_at,
    "duration_ms": duration_ms,
    "close_reason": close_reason,
})
```

- [ ] **Step 3: Add docker logs subscribe/unsubscribe audit logging**

In `crates/server/src/router/ws/docker_logs.rs`:

- capture `user_id` and `ip`
- when `Subscribe` succeeds, write `docker_logs_subscribed` and insert context
- when `Subscribe` is blocked by capability, write `docker_logs_subscribe_denied`
- on cleanup, remove the context and write `docker_logs_unsubscribed`

- [ ] **Step 4: Expand file audits without breaking existing write coverage**

In `crates/server/src/router/api/file.rs`:

- keep existing `file_write`, `file_delete`, `file_mkdir`, `file_move`, `file_upload`
- convert any remaining ad hoc detail strings to stable JSON keys
- add `file_read` / `file_read_denied`
- move `file_download` to the stream-start path if that yields a more truthful “actual export happened” moment; otherwise document the chosen semantics in the handler comment and keep it consistent

- [ ] **Step 5: Add docker REST view audits**

In `crates/server/src/router/api/docker.rs`, write:

- `docker_view` on successful GETs (`containers`, `stats`, `info`, `events`, `networks`, `volumes`)
- `docker_view_denied` when capability blocks access

Include `resource` and `server_id` in every detail payload.

- [ ] **Step 6: Add exec lifecycle audits**

In `crates/server/src/router/api/task.rs`:

- write `exec_started` when dispatching to a server
- write `exec_denied` when server or agent-local capability blocks dispatch

In `crates/server/src/router/ws/agent.rs`, when handling `TaskResult`, write `exec_finished` with:

- `server_id`
- `task_id`
- `exit_code`
- `command` (recovered from pending dispatch metadata if available)

If the command text is not currently recoverable at finish time, add the minimum pending metadata needed rather than dropping the audit field.

- [ ] **Step 7: Verify audit behavior**

Run: `cargo test -p serverbee-server --test integration audit`
Expected: PASS

Run: `cargo test -p serverbee-server --test docker_integration audit`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/router/ws/terminal.rs crates/server/src/router/ws/docker_logs.rs crates/server/src/router/api/file.rs crates/server/src/router/api/docker.rs crates/server/src/router/api/task.rs crates/server/src/router/ws/agent.rs
git commit -m "feat(server): audit high-risk user reads and session lifecycles"
```

---

## Chunk 4: Frontend, Generated Types, Docs, And Final Verification

### Task 8: Update web capability state handling and disable client-locked toggles

**Files:**
- Modify: `apps/web/src/lib/capabilities.ts`
- Modify: `apps/web/src/lib/capabilities.test.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.test.ts`
- Modify: `apps/web/src/routes/_authed/settings/capabilities.tsx`
- Modify: `apps/web/src/components/server/capabilities-dialog.tsx`
- Modify: `apps/web/src/components/server/capabilities-dialog.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$serverId/docker/index.tsx`
- Modify: `apps/web/src/routes/_authed/settings/tasks.tsx`
- Modify: `apps/web/src/components/task/scheduled-task-dialog.tsx`

- [ ] **Step 1: Add frontend helpers and tests before touching UI components**

In `apps/web/src/lib/capabilities.ts`, add helper functions such as:

```ts
export function isClientCapabilityLocked(
  agentLocalCapabilities: number | null | undefined,
  bit: number
): boolean { ... }

export function getEffectiveCapabilityEnabled(
  effectiveCapabilities: number | null | undefined,
  configuredCapabilities: number | null | undefined,
  bit: number
): boolean { ... }
```

Add tests in `apps/web/src/lib/capabilities.test.ts` for:

- client lock true/false
- effective bitmap fallback behavior
- server configured on + client locked off => effective false

Run: `cd apps/web && bun run test -- src/lib/capabilities.test.ts`
Expected: FAIL until helpers are implemented

- [ ] **Step 2: Teach the WS cache about the new capability fields**

Update `apps/web/src/hooks/use-servers-ws.ts`:

- extend `ServerMetrics`
- extend `WsMessage['capabilities_changed']`
- update `handleCapabilityMessage()` to set:
  - `capabilities`
  - `agent_local_capabilities`
  - `effective_capabilities`

Extend `apps/web/src/hooks/use-servers-ws.test.ts` with a case that applies a capability update and asserts all three fields land in cache shape.

- [ ] **Step 3: Disable capability toggles when the client locks them**

Update both capability management UIs:

- `apps/web/src/routes/_authed/settings/capabilities.tsx`
- `apps/web/src/components/server/capabilities-dialog.tsx`

Behavior:

- `Switch.disabled = mutation.isPending || isClientLocked`
- tooltip/title shows `客户端关闭`
- visual state uses `effective_capabilities` instead of only `capabilities`

Update the dialog test to assert a locked capability renders disabled and shows the tooltip text.

- [ ] **Step 4: Replace direct `capabilities` checks in other pages**

Update direct bit checks in:

- `apps/web/src/routes/_authed/servers/$id.tsx`
- `apps/web/src/routes/_authed/servers/$serverId/docker/index.tsx`
- `apps/web/src/routes/_authed/settings/tasks.tsx`
- `apps/web/src/components/task/scheduled-task-dialog.tsx`

Use the helper functions so terminal/file/docker/exec availability matches the same effective-capability semantics as the settings page.

- [ ] **Step 5: Regenerate OpenAPI-derived web types**

Run: `make web-generate-api-types`
Expected: `apps/web/openapi.json` and `apps/web/src/lib/api-types.ts` update with the new `ServerResponse` fields

- [ ] **Step 6: Verify the web app**

Run: `cd apps/web && bun run test -- src/lib/capabilities.test.ts src/hooks/use-servers-ws.test.ts src/components/server/capabilities-dialog.test.tsx`
Expected: PASS

Run: `cd apps/web && bun run typecheck`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/lib/capabilities.ts apps/web/src/lib/capabilities.test.ts apps/web/src/hooks/use-servers-ws.ts apps/web/src/hooks/use-servers-ws.test.ts apps/web/src/routes/_authed/settings/capabilities.tsx apps/web/src/components/server/capabilities-dialog.tsx apps/web/src/components/server/capabilities-dialog.test.tsx 'apps/web/src/routes/_authed/servers/$id.tsx' 'apps/web/src/routes/_authed/servers/$serverId/docker/index.tsx' apps/web/src/routes/_authed/settings/tasks.tsx apps/web/src/components/task/scheduled-task-dialog.tsx apps/web/openapi.json apps/web/src/lib/api-types.ts
git commit -m "feat(web): show effective capabilities and disable client-locked controls"
```

---

### Task 9: Update public docs and run final verification

**Files:**
- Modify: `apps/docs/content/docs/en/agent.mdx`
- Modify: `apps/docs/content/docs/cn/agent.mdx`
- Modify: `apps/docs/content/docs/en/capabilities.mdx`
- Modify: `apps/docs/content/docs/cn/capabilities.mdx`

- [ ] **Step 1: Document the new agent CLI flags**

In `apps/docs/content/docs/en/agent.mdx` and `apps/docs/content/docs/cn/agent.mdx`, add a small section showing:

```bash
serverbee-agent --allow-cap terminal --allow-cap exec
serverbee-agent --deny-cap ping_http
```

Explain:

- low-risk defaults are enabled
- high-risk defaults are disabled
- `--deny-cap` wins over `--allow-cap`

- [ ] **Step 2: Document the UI lock semantics**

In `apps/docs/content/docs/en/capabilities.mdx` and `apps/docs/content/docs/cn/capabilities.mdx`, add:

- server-configured vs client-allowed explanation
- “客户端关闭” disabled-state meaning
- note that the server cannot override client-local denies

- [ ] **Step 3: Run Rust formatting and linting**

Run: `cargo fmt`
Expected: no diff formatting errors remain

Run: `cargo clippy --all --benches --tests --examples --all-features`
Expected: PASS with no warnings

- [ ] **Step 4: Run the focused Rust test set**

Run:

```bash
cargo test -p serverbee-common
cargo test -p serverbee-agent capability_policy
cargo test -p serverbee-agent reporter
cargo test -p serverbee-server --test integration capability
cargo test -p serverbee-server --test integration audit
cargo test -p serverbee-server --test docker_integration capability
cargo test -p serverbee-server --test docker_integration audit
```

Expected: PASS

- [ ] **Step 5: Run the focused web test set**

Run:

```bash
cd apps/web && bun run test -- src/lib/capabilities.test.ts src/hooks/use-servers-ws.test.ts src/components/server/capabilities-dialog.test.tsx
```

Expected: PASS

Run: `cd apps/web && bun run typecheck`
Expected: PASS

- [ ] **Step 6: Commit docs and final cleanups**

```bash
git add apps/docs/content/docs/en/agent.mdx apps/docs/content/docs/cn/agent.mdx apps/docs/content/docs/en/capabilities.mdx apps/docs/content/docs/cn/capabilities.mdx
git commit -m "docs: explain agent local capability locks and UI behavior"
```

- [ ] **Step 7: Final handoff checklist**

Before claiming completion, confirm all of the following:

- `agent_local_capabilities` is returned by `/api/servers` and `/api/servers/{id}`
- `effective_capabilities` is used everywhere the UI decides capability availability
- client-locked toggles are disabled with the correct tooltip
- file write/delete/mkdir/move/upload audits still exist and use JSON detail
- new read/session audits are present only for user-initiated flows
- OpenAPI output and generated TypeScript types are committed

---

Plan complete and saved to `docs/superpowers/plans/2026-04-12-agent-local-capability-locks-and-audit.md`. Ready to execute?
