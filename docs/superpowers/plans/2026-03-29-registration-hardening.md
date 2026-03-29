# Registration Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent server table flooding from duplicate agent registrations and leaked discovery keys.

**Architecture:** Agent sends a SHA-256 fingerprint (hostname + machine_id) during registration. Server deduplicates by fingerprint, reusing existing server records. Global server count soft-cap prevents runaway growth. Dashboard gets a "Regenerate" button for discovery key and a "Clean up" button for orphaned servers.

**Tech Stack:** Rust (Axum, sea-orm, sha2), React (TanStack Query, shadcn/ui), SQLite migration

**Spec:** `docs/superpowers/specs/2026-03-29-registration-hardening-design.md`

---

### Task 1: Database Migration — Add fingerprint column

**Files:**
- Create: `crates/server/src/migration/m20260329_000013_add_server_fingerprint.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Modify: `crates/server/src/entity/server.rs`

- [ ] **Step 1: Create migration file**

Create `crates/server/src/migration/m20260329_000013_add_server_fingerprint.rs`:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260329_000013_add_server_fingerprint"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN fingerprint VARCHAR NULL",
        )
        .await?;

        db.execute_unprepared(
            "CREATE UNIQUE INDEX idx_servers_fingerprint ON servers(fingerprint) WHERE fingerprint IS NOT NULL",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration in mod.rs**

In `crates/server/src/migration/mod.rs`, add the new module and register it:

```rust
mod m20260329_000013_add_server_fingerprint;
```

Add to the `migrations()` vec:

```rust
Box::new(m20260329_000013_add_server_fingerprint::Migration),
```

- [ ] **Step 3: Add fingerprint field to server entity**

In `crates/server/src/entity/server.rs`, add after `last_remote_addr`:

```rust
    pub fingerprint: Option<String>,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles successfully (server crate has RustEmbed issue unrelated to our change)

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration/ crates/server/src/entity/server.rs
git commit -m "feat(db): add fingerprint column to servers table"
```

---

### Task 2: Agent Fingerprint Module

**Files:**
- Create: `crates/agent/src/fingerprint.rs`
- Modify: `crates/agent/src/main.rs`

- [ ] **Step 1: Create fingerprint module**

Create `crates/agent/src/fingerprint.rs`:

```rust
use sha2::{Digest, Sha256};

/// Generate a machine fingerprint: SHA-256 of "{hostname}:{machine_id}".
/// Returns empty string if machine_id is unavailable (caller should skip fingerprint).
pub fn generate() -> String {
    let machine_id = match read_machine_id() {
        Some(id) => id,
        None => {
            tracing::warn!("Could not read machine-id, fingerprint will be skipped");
            return String::new();
        }
    };

    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .to_string();

    let input = format!("{hostname}:{machine_id}");
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(hash)
}

#[cfg(target_os = "linux")]
fn read_machine_id() -> Option<String> {
    std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "macos")]
fn read_machine_id() -> Option<String> {
    let output = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            // Line format: "IOPlatformUUID" = "XXXXXXXX-XXXX-..."
            return line.split('"').nth(3).map(|s| s.to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn read_machine_id() -> Option<String> {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output format: "    MachineGuid    REG_SZ    XXXXXXXX-XXXX-..."
    for line in stdout.lines() {
        if line.contains("MachineGuid") {
            return line.split_whitespace().last().map(|s| s.to_string());
        }
    }
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn read_machine_id() -> Option<String> {
    None
}
```

- [ ] **Step 2: Add dependencies to agent Cargo.toml**

In `crates/agent/Cargo.toml`, add (sha2 already exists, need hex and gethostname):

```toml
hex = "0.4"
gethostname = "0.5"
```

- [ ] **Step 3: Register module in main.rs**

In `crates/agent/src/main.rs`, add after `mod file_manager;`:

```rust
mod fingerprint;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add crates/agent/
git commit -m "feat(agent): add machine fingerprint module"
```

---

### Task 3: Agent Registration Protocol — Send Fingerprint

**Files:**
- Modify: `crates/agent/src/register.rs`
- Modify: `crates/agent/src/main.rs`
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Update RegisterRequest to include fingerprint**

In `crates/agent/src/register.rs`, update `RegisterRequest`:

```rust
#[derive(Serialize)]
struct RegisterRequest {
    #[serde(skip_serializing_if = "String::is_empty")]
    fingerprint: String,
}
```

Update `register_agent` to accept fingerprint:

```rust
pub async fn register_agent(config: &AgentConfig, fingerprint: &str) -> Result<(String, String)> {
    let url = format!(
        "{}/api/agent/register",
        config.server_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&config.auto_discovery_key)
        .json(&RegisterRequest {
            fingerprint: fingerprint.to_string(),
        })
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Registration failed: HTTP {}", resp.status());
    }
    let data: RegisterResponse = resp.json().await?;
    Ok((data.data.server_id, data.data.token))
}
```

- [ ] **Step 2: Update main.rs to generate and pass fingerprint**

In `crates/agent/src/main.rs`, update the registration block (lines 37-48):

```rust
    let machine_fingerprint = fingerprint::generate();
    if !machine_fingerprint.is_empty() {
        tracing::info!("Machine fingerprint: {}...{}", &machine_fingerprint[..8], &machine_fingerprint[56..]);
    }

    if config.token.is_empty() {
        if config.auto_discovery_key.is_empty() {
            anyhow::bail!("No token and no auto_discovery_key. Set one in config.");
        }
        tracing::info!("No token found, registering...");
        let (_server_id, token) = register::register_agent(&config, &machine_fingerprint).await?;
        tracing::info!("Registration successful");
        if let Err(e) = register::save_token(&token) {
            tracing::warn!("Failed to save token: {e}");
        }
        config.token = token;
    }
```

- [ ] **Step 3: Update reporter.rs re-registration call**

In `crates/agent/src/reporter.rs`, the `Reporter` struct needs access to the fingerprint. Update the struct and `new()`:

```rust
pub struct Reporter {
    config: AgentConfig,
    fingerprint: String,
}

impl Reporter {
    pub fn new(config: AgentConfig, fingerprint: String) -> Self {
        Self { config, fingerprint }
    }
```

Update the re-registration call in `run()` (where `register::register_agent` is called):

```rust
match register::register_agent(&self.config, &self.fingerprint).await {
```

Update `main.rs` to pass fingerprint to Reporter:

```rust
    let mut reporter = Reporter::new(config, machine_fingerprint);
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/
git commit -m "feat(agent): send fingerprint during registration"
```

---

### Task 4: Server Config — Add max_servers

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Add max_servers to AuthConfig**

In `crates/server/src/config.rs`, add to `AuthConfig` struct after `secure_cookie`:

```rust
    /// Maximum number of servers allowed (0 = no limit, best-effort soft cap).
    #[serde(default)]
    pub max_servers: u32,
```

Update `Default for AuthConfig`:

```rust
impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_ttl: default_session_ttl(),
            auto_discovery_key: String::new(),
            secure_cookie: true,
            max_servers: 0,
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles successfully

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat(config): add auth.max_servers setting"
```

---

### Task 5: Server Registration — Fingerprint Dedup + Global Limit

**Files:**
- Modify: `crates/server/src/router/api/agent.rs`

- [ ] **Step 1: Rewrite registration handler with dedup logic**

Replace the full contents of `crates/server/src/router/api/agent.rs`:

```rust
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::server;
use crate::error::{ok, ApiResponse, AppError};
use crate::router::utils::extract_client_ip;
use crate::service::auth::AuthService;
use crate::service::config::ConfigService;
use crate::service::network_probe::NetworkProbeService;
use crate::state::AppState;

const CONFIG_KEY_AUTO_DISCOVERY: &str = "auto_discovery_key";
const DEFAULT_SERVER_NAME: &str = "New Server";

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    fingerprint: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    server_id: String,
    token: String,
}

/// Public routes for agent registration (Bearer auth checked inside handler).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/register", post(register))
}

#[utoipa::path(
    post,
    path = "/api/agent/register",
    tag = "agent",
    responses(
        (status = 200, description = "Agent registered", body = RegisterResponse),
        (status = 400, description = "Auto-discovery key not configured or server limit reached"),
        (status = 401, description = "Invalid auto-discovery key"),
    ),
    security(("bearer_token" = []))
)]
async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Option<Json<RegisterRequest>>,
) -> Result<Json<ApiResponse<RegisterResponse>>, AppError> {
    // 1. Rate limiting
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    if !state.check_register_rate(&ip) {
        return Err(AppError::TooManyRequests(
            "Too many registration attempts, please try later".to_string(),
        ));
    }

    // 2. Discovery key validation
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let stored_key = ConfigService::get(&state.db, CONFIG_KEY_AUTO_DISCOVERY)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest("Auto-discovery key not configured".to_string())
        })?;

    if stored_key.is_empty() {
        return Err(AppError::BadRequest(
            "Auto-discovery key not configured".to_string(),
        ));
    }

    if auth_header != stored_key {
        return Err(AppError::Unauthorized);
    }

    let fingerprint = body
        .as_ref()
        .map(|b| b.fingerprint.clone())
        .unwrap_or_default();

    // 3. Fingerprint dedup: try to reuse existing server
    if !fingerprint.is_empty() {
        if let Some(existing) = server::Entity::find()
            .filter(server::Column::Fingerprint.eq(&fingerprint))
            .one(&state.db)
            .await?
        {
            let server_id = existing.id.clone();
            tracing::info!("Reusing server {server_id} for fingerprint {fingerprint}");

            let plaintext_token = AuthService::generate_session_token();
            let token_hash = AuthService::hash_password(&plaintext_token)?;
            let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];

            let mut active: server::ActiveModel = existing.into();
            active.token_hash = Set(token_hash);
            active.token_prefix = Set(token_prefix.to_string());
            active.last_remote_addr = Set(Some(ip));
            active.updated_at = Set(Utc::now());
            active.update(&state.db).await?;

            return ok(RegisterResponse {
                server_id,
                token: plaintext_token,
            });
        }
    }

    // 4. Global server limit check (soft cap, only for new servers)
    let max_servers = state.config.auth.max_servers;
    if max_servers > 0 {
        let count = server::Entity::find().count(&state.db).await?;
        if count >= max_servers as u64 {
            return Err(AppError::BadRequest(format!(
                "Server limit reached ({max_servers}). Delete unused servers or increase max_servers in config."
            )));
        }
    }

    // 5. Create new server
    let server_id = Uuid::new_v4().to_string();
    let plaintext_token = AuthService::generate_session_token();
    let token_hash = AuthService::hash_password(&plaintext_token)?;
    let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];
    let now = Utc::now();

    let fp = if fingerprint.is_empty() {
        None
    } else {
        Some(fingerprint.clone())
    };

    let new_server = server::ActiveModel {
        id: Set(server_id.clone()),
        token_hash: Set(token_hash),
        token_prefix: Set(token_prefix.to_string()),
        name: Set(DEFAULT_SERVER_NAME.to_string()),
        cpu_name: Set(None),
        cpu_cores: Set(None),
        cpu_arch: Set(None),
        os: Set(None),
        kernel_version: Set(None),
        mem_total: Set(None),
        swap_total: Set(None),
        disk_total: Set(None),
        ipv4: Set(None),
        ipv6: Set(None),
        region: Set(None),
        country_code: Set(None),
        virtualization: Set(None),
        agent_version: Set(None),
        group_id: Set(None),
        weight: Set(0),
        hidden: Set(false),
        remark: Set(None),
        public_remark: Set(None),
        price: Set(None),
        billing_cycle: Set(None),
        currency: Set(None),
        expired_at: Set(None),
        traffic_limit: Set(None),
        traffic_limit_type: Set(None),
        billing_start_day: Set(None),
        capabilities: Set(56),
        protocol_version: Set(1),
        features: Set("[]".to_string()),
        last_remote_addr: Set(Some(ip)),
        fingerprint: Set(fp.clone()),
        created_at: Set(now),
        updated_at: Set(now),
    };

    // Handle race condition: if another request with the same fingerprint inserted
    // between our SELECT and INSERT, catch the unique constraint violation and retry as reuse.
    match new_server.insert(&state.db).await {
        Ok(_) => {}
        Err(DbErr::Query(ref e)) if fp.is_some() && e.to_string().contains("UNIQUE") => {
            tracing::info!("Fingerprint race detected, falling back to reuse path");
            if let Some(existing) = server::Entity::find()
                .filter(server::Column::Fingerprint.eq(fp.as_ref().unwrap()))
                .one(&state.db)
                .await?
            {
                let server_id = existing.id.clone();
                let plaintext_token = AuthService::generate_session_token();
                let token_hash = AuthService::hash_password(&plaintext_token)?;
                let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];

                let mut active: server::ActiveModel = existing.into();
                active.token_hash = Set(token_hash);
                active.token_prefix = Set(token_prefix.to_string());
                active.updated_at = Set(Utc::now());
                active.update(&state.db).await?;

                return ok(RegisterResponse {
                    server_id,
                    token: plaintext_token,
                });
            }
            return Err(AppError::Internal("Fingerprint race recovery failed".to_string()));
        }
        Err(e) => return Err(e.into()),
    }

    // Apply default network probe targets
    if let Err(e) = NetworkProbeService::apply_defaults(&state.db, &server_id).await {
        tracing::warn!("Failed to apply default network probe targets to {server_id}: {e}");
    }

    ok(RegisterResponse {
        server_id,
        token: plaintext_token,
    })
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles successfully

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/api/agent.rs
git commit -m "feat(register): fingerprint dedup, global limit, race handling"
```

---

### Task 6: Server Cleanup Endpoint

**Files:**
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Add cleanup endpoint to server.rs**

In `crates/server/src/router/api/server.rs`, add at the top with other imports:

```rust
use sea_orm::sea_query::Expr;
```

Add the response type near the other structs:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CleanupResponse {
    deleted_count: u64,
}
```

Add the route to `write_router()`:

```rust
.route("/servers/cleanup", delete(cleanup_orphaned_servers))
```

Add the handler function:

```rust
#[utoipa::path(
    delete,
    path = "/api/servers/cleanup",
    tag = "servers",
    responses(
        (status = 200, description = "Orphaned servers cleaned up", body = CleanupResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn cleanup_orphaned_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<CleanupResponse>>, AppError> {
    use crate::entity::*;

    // Find orphaned servers: name = "New Server" AND os IS NULL
    let orphans = server::Entity::find()
        .filter(server::Column::Name.eq("New Server"))
        .filter(server::Column::Os.is_null())
        .all(&state.db)
        .await?;

    let orphan_ids: Vec<String> = orphans.iter().map(|s| s.id.clone()).collect();
    if orphan_ids.is_empty() {
        return ok(CleanupResponse { deleted_count: 0 });
    }

    let txn = state.db.begin().await?;

    // Tables with server_id FK — delete rows
    record::Entity::delete_many()
        .filter(record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    record_hourly::Entity::delete_many()
        .filter(record_hourly::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    gpu_record::Entity::delete_many()
        .filter(gpu_record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    alert_state::Entity::delete_many()
        .filter(alert_state::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    network_probe_config::Entity::delete_many()
        .filter(network_probe_config::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    network_probe_record::Entity::delete_many()
        .filter(network_probe_record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    network_probe_record_hourly::Entity::delete_many()
        .filter(network_probe_record_hourly::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    traffic_state::Entity::delete_many()
        .filter(traffic_state::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    traffic_hourly::Entity::delete_many()
        .filter(traffic_hourly::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    traffic_daily::Entity::delete_many()
        .filter(traffic_daily::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    uptime_daily::Entity::delete_many()
        .filter(uptime_daily::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    task_result::Entity::delete_many()
        .filter(task_result::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    server_tag::Entity::delete_many()
        .filter(server_tag::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    docker_event::Entity::delete_many()
        .filter(docker_event::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;
    ping_record::Entity::delete_many()
        .filter(ping_record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn).await?;

    // Tables with server_ids_json — remove orphan IDs from JSON arrays
    cleanup_json_array_tables(&txn, &orphan_ids).await?;

    // Delete orphaned servers
    let deleted = server::Entity::delete_many()
        .filter(server::Column::Id.is_in(&orphan_ids))
        .exec(&txn)
        .await?;

    txn.commit().await?;

    tracing::info!("Cleaned up {} orphaned servers", deleted.rows_affected);
    ok(CleanupResponse {
        deleted_count: deleted.rows_affected,
    })
}

/// Remove orphan server IDs from server_ids_json columns in shared-config tables.
/// Per-table rules for what to do when the array becomes empty after removal.
async fn cleanup_json_array_tables(
    txn: &sea_orm::DatabaseTransaction,
    orphan_ids: &[String],
) -> Result<(), AppError> {
    use crate::entity::*;
    use sea_orm::ConnectionTrait;

    // Helper: for each table, load all rows, filter those containing orphan IDs,
    // update or delete per the per-table rule.

    // ping_tasks: delete if empty
    for task in ping_task::Entity::find().all(txn).await? {
        if let Some(new_json) = remove_ids_from_json(&task.server_ids_json, orphan_ids) {
            if new_json == "[]" {
                ping_task::Entity::delete_by_id(task.id).exec(txn).await?;
            } else {
                let mut active: ping_task::ActiveModel = task.into();
                active.server_ids_json = Set(new_json);
                active.update(txn).await?;
            }
        }
    }

    // tasks: delete if empty
    for task in task::Entity::find().all(txn).await? {
        if let Some(new_json) = remove_ids_from_json(&task.server_ids_json, orphan_ids) {
            if new_json == "[]" {
                task::Entity::delete_by_id(task.id).exec(txn).await?;
            } else {
                let mut active: task::ActiveModel = task.into();
                active.server_ids_json = Set(new_json);
                active.update(txn).await?;
            }
        }
    }

    // alert_rules: delete if empty
    for rule in alert_rule::Entity::find().all(txn).await? {
        if let Some(ref json) = rule.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                if new_json == "[]" {
                    // Also delete related alert_states
                    alert_state::Entity::delete_many()
                        .filter(alert_state::Column::RuleId.eq(rule.id))
                        .exec(txn).await?;
                    alert_rule::Entity::delete_by_id(rule.id).exec(txn).await?;
                } else {
                    let mut active: alert_rule::ActiveModel = rule.into();
                    active.server_ids_json = Set(Some(new_json));
                    active.update(txn).await?;
                }
            }
        }
    }

    // maintenances: delete if empty
    for m in maintenance::Entity::find().all(txn).await? {
        if let Some(ref json) = m.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                if new_json == "[]" {
                    maintenance::Entity::delete_by_id(m.id).exec(txn).await?;
                } else {
                    let mut active: maintenance::ActiveModel = m.into();
                    active.server_ids_json = Set(Some(new_json));
                    active.update(txn).await?;
                }
            }
        }
    }

    // service_monitors: set to NULL if empty (preserve monitor + history)
    for monitor in service_monitor::Entity::find().all(txn).await? {
        if let Some(ref json) = monitor.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                let mut active: service_monitor::ActiveModel = monitor.into();
                if new_json == "[]" {
                    active.server_ids_json = Set(None);
                } else {
                    active.server_ids_json = Set(Some(new_json));
                }
                active.update(txn).await?;
            }
        }
    }

    // incidents: keep row, just update array
    for incident in incident::Entity::find().all(txn).await? {
        if let Some(ref json) = incident.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                let mut active: incident::ActiveModel = incident.into();
                active.server_ids_json = Set(Some(new_json));
                active.update(txn).await?;
            }
        }
    }

    // status_pages: keep row, just update array
    for page in status_page::Entity::find().all(txn).await? {
        if let Some(new_json) = remove_ids_from_json(&page.server_ids_json, orphan_ids) {
            let mut active: status_page::ActiveModel = page.into();
            active.server_ids_json = Set(new_json);
            active.update(txn).await?;
        }
    }

    Ok(())
}

/// Remove orphan_ids from a JSON array string. Returns Some(new_json) if any were removed, None if unchanged.
fn remove_ids_from_json(json: &str, orphan_ids: &[String]) -> Option<String> {
    let ids: Vec<String> = serde_json::from_str(json).unwrap_or_default();
    let filtered: Vec<&String> = ids.iter().filter(|id| !orphan_ids.contains(id)).collect();
    if filtered.len() == ids.len() {
        return None; // No change
    }
    Some(serde_json::to_string(&filtered).unwrap_or_else(|_| "[]".to_string()))
}
```

- [ ] **Step 2: Register in OpenAPI**

In `crates/server/src/openapi.rs`, add in the `paths(...)` section under the servers block:

```rust
crate::router::api::server::cleanup_orphaned_servers,
```

Add in the `components(schemas(...))` section:

```rust
crate::router::api::server::CleanupResponse,
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/server.rs crates/server/src/openapi.rs
git commit -m "feat(api): add DELETE /servers/cleanup endpoint"
```

---

### Task 7: Frontend — Regenerate Discovery Key Button

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/index.tsx`
- Modify: `apps/web/src/locales/en/settings.json`
- Modify: `apps/web/src/locales/zh/settings.json`

- [ ] **Step 1: Add i18n keys**

In `apps/web/src/locales/en/settings.json`, add after `"copy_key": "Copy key"`:

```json
  "regenerate_key": "Regenerate",
  "regenerate_confirm_title": "Regenerate Discovery Key?",
  "regenerate_confirm_description": "This will invalidate the current key. Already connected agents are not affected. New agents must use the new key.",
  "regenerate_success": "Discovery key regenerated",
```

In `apps/web/src/locales/zh/settings.json`, add after `"copy_key": "复制密钥"`:

```json
  "regenerate_key": "重新生成",
  "regenerate_confirm_title": "重新生成发现密钥？",
  "regenerate_confirm_description": "这将使当前密钥失效。已连接的 Agent 不受影响。新 Agent 必须使用新密钥。",
  "regenerate_success": "发现密钥已重新生成",
```

- [ ] **Step 2: Add Regenerate button to settings page**

Update `apps/web/src/routes/_authed/settings/index.tsx`:

```tsx
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Copy, Eye, EyeOff, RefreshCw } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { GeoIpCard } from '@/components/settings/geoip-card'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { AutoDiscoveryKeyResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/')({
  component: SettingsPage
})

function SettingsPage() {
  const { t } = useTranslation('settings')
  const [showKey, setShowKey] = useState(false)
  const queryClient = useQueryClient()

  const { data: config } = useQuery<AutoDiscoveryKeyResponse>({
    queryKey: ['settings', 'discovery'],
    queryFn: () => api.get<AutoDiscoveryKeyResponse>('/api/settings/auto-discovery-key')
  })

  const regenerateMutation = useMutation({
    mutationFn: () => api.put<AutoDiscoveryKeyResponse>('/api/settings/auto-discovery-key'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings', 'discovery'] })
      setShowKey(true)
      toast.success(t('regenerate_success'))
    }
  })

  const handleCopy = async () => {
    if (!config?.key) {
      return
    }
    try {
      await navigator.clipboard.writeText(config.key)
      toast.success('Copied to clipboard')
    } catch {
      // Clipboard access denied
    }
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('title')}</h1>

      <div className="max-w-xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <h2 className="mb-1 font-semibold text-lg">{t('auto_discovery_key')}</h2>
          <p className="mb-4 text-muted-foreground text-sm">{t('auto_discovery_description')}</p>

          {config?.key ? (
            <div className="flex items-center gap-2">
              <div className="flex-1 rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                {showKey ? config.key : config.key.replace(/./g, '*')}
              </div>
              <Button
                aria-label={showKey ? t('hide_key') : t('show_key')}
                onClick={() => setShowKey((prev) => !prev)}
                size="icon"
                variant="outline"
              >
                {showKey ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
              </Button>
              <Button aria-label={t('copy_key')} onClick={handleCopy} size="icon" variant="outline">
                <Copy className="size-4" />
              </Button>
              <AlertDialog>
                <AlertDialogTrigger
                  render={
                    <Button
                      aria-label={t('regenerate_key')}
                      disabled={regenerateMutation.isPending}
                      size="icon"
                      variant="outline"
                    >
                      <RefreshCw className={`size-4 ${regenerateMutation.isPending ? 'animate-spin' : ''}`} />
                    </Button>
                  }
                />
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>{t('regenerate_confirm_title')}</AlertDialogTitle>
                    <AlertDialogDescription>{t('regenerate_confirm_description')}</AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                    <AlertDialogAction onClick={() => regenerateMutation.mutate()} variant="destructive">
                      {t('regenerate_key')}
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          ) : (
            <Skeleton className="h-10 rounded-md" />
          )}
        </div>

        <GeoIpCard />
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Verify frontend builds**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/settings/index.tsx apps/web/src/locales/
git commit -m "feat(ui): add discovery key regenerate button"
```

---

### Task 8: Frontend — Cleanup Orphaned Servers Button

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`
- Modify: `apps/web/src/locales/en/servers.json`
- Modify: `apps/web/src/locales/zh/servers.json`

- [ ] **Step 1: Add i18n keys**

In `apps/web/src/locales/en/servers.json`, add after `"selected_count"`:

```json
  "cleanup_orphans": "Clean up unconnected",
  "cleanup_confirm_title": "Clean up unconnected servers?",
  "cleanup_confirm_description": "This will delete {{count}} server(s) that registered but never connected. This cannot be undone.",
  "cleanup_success": "Cleaned up {{count}} server(s)",
  "cleanup_none": "No unconnected servers to clean up",
```

In `apps/web/src/locales/zh/servers.json`, add after `"selected_count"`:

```json
  "cleanup_orphans": "清理未连接",
  "cleanup_confirm_title": "清理未连接的服务器？",
  "cleanup_confirm_description": "将删除 {{count}} 台注册后从未连接的服务器，此操作不可撤销。",
  "cleanup_success": "已清理 {{count}} 台服务器",
  "cleanup_none": "没有需要清理的未连接服务器",
```

- [ ] **Step 2: Add cleanup button to server list page**

In `apps/web/src/routes/_authed/servers/index.tsx`, add the cleanup button and mutation. The button goes in the toolbar area (after the search input, before the batch delete AlertDialog).

Add the import for `Trash` icon if not already present, and add the cleanup query and mutation:

```tsx
const orphanCount = servers.filter((s) => s.name === 'New Server' && !s.os).length

const cleanupMutation = useMutation({
  mutationFn: () => api.delete<{ deleted_count: number }>('/api/servers/cleanup'),
  onSuccess: (data) => {
    queryClient.invalidateQueries({ queryKey: ['servers'] })
    toast.success(t('servers:cleanup_success', { count: data.deleted_count }))
  }
})
```

Add the cleanup button in the toolbar, before the batch delete AlertDialog:

```tsx
{orphanCount > 0 && (
  <AlertDialog>
    <AlertDialogTrigger
      render={
        <Button disabled={cleanupMutation.isPending} size="sm" variant="outline">
          {t('servers:cleanup_orphans')} ({orphanCount})
        </Button>
      }
    />
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{t('servers:cleanup_confirm_title')}</AlertDialogTitle>
        <AlertDialogDescription>
          {t('servers:cleanup_confirm_description', { count: orphanCount })}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
        <AlertDialogAction onClick={() => cleanupMutation.mutate()} variant="destructive">
          {t('common:delete')}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
)}
```

- [ ] **Step 3: Verify frontend builds**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.tsx apps/web/src/locales/
git commit -m "feat(ui): add cleanup unconnected servers button"
```

---

### Task 9: Documentation Updates

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`
- Modify: `deploy/railway/README.md`

- [ ] **Step 1: Update ENV.md**

Add after the `SERVERBEE_AUTH__AUTO_DISCOVERY_KEY` row in the Common table:

```markdown
| `SERVERBEE_AUTH__MAX_SERVERS` | `auth.max_servers` | u32 | `0` | Maximum servers allowed via auto-discovery (0 = no limit). Best-effort soft cap |
```

- [ ] **Step 2: Update English configuration.mdx**

In `apps/docs/content/docs/en/configuration.mdx`, add `SERVERBEE_AUTH__MAX_SERVERS` to the auth variables table.

Add a Docker agent fingerprint note in the agent section:

```markdown
> **Docker Agent:** Mount the host's machine-id for correct fingerprint identification:
> ```
> -v /etc/machine-id:/etc/machine-id:ro
> ```
```

- [ ] **Step 3: Update Chinese configuration.mdx**

Same changes as English, translated to Chinese:

```markdown
> **Docker Agent：** 挂载宿主机的 machine-id 以确保指纹识别正确：
> ```
> -v /etc/machine-id:/etc/machine-id:ro
> ```
```

- [ ] **Step 4: Update Railway README**

In `deploy/railway/README.md`, add `MAX_SERVERS` to the environment variables section.

- [ ] **Step 5: Commit**

```bash
git add ENV.md apps/docs/ deploy/railway/README.md
git commit -m "docs: add max_servers env var and Docker machine-id mount"
```

---

### Task 10: Drop stashed WIP changes

The earlier direct implementation attempt was stashed. It should be dropped since this plan supersedes it.

- [ ] **Step 1: Drop the stash**

```bash
git stash drop
```

- [ ] **Step 2: Push all changes**

```bash
git push
```
