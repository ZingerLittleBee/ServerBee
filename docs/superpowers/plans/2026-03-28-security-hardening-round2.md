# Security Hardening Round 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 5 security/quality issues: batch capabilities transaction, security response headers, file upload size limit, auth/me argon2 caching, and external IP response size limit.

**Architecture:** All fixes are independent and can be implemented in any order. Each targets a specific file with minimal cross-cutting concerns. F4 (auth/me caching) is the only fix requiring a database migration.

**Tech Stack:** Rust (Axum 0.8, sea-orm 1.x, tower-http 0.6), reqwest (agent)

**Design Spec:** `docs/superpowers/specs/2026-03-28-security-hardening-round2-design.md`

---

### Task 1: F1 — batch_update_capabilities transaction protection

**Files:**
- Modify: `crates/server/src/router/api/server.rs:602-722`

- [ ] **Step 1: Define CapabilityChangeEffect struct**

Add a struct above the `batch_update_capabilities` function to collect side effects:

```rust
/// Side effects to execute after transaction commit.
struct CapabilityChangeEffect {
    server_id: String,
    old_caps: u32,
    new_caps: u32,
}
```

Add this at line ~598 (above the `#[utoipa::path]` annotation for `batch_update_capabilities`).

- [ ] **Step 2: Wrap DB updates in transaction, collect side effects**

Replace lines 637-719 (the `for` loop and everything inside it) with the two-phase approach:

```rust
    let mut count = 0u64;
    let mut effects: Vec<CapabilityChangeEffect> = Vec::new();

    // Phase 1: All DB updates in a single transaction
    let txn = state.db.begin().await?;
    for s in &servers {
        let old_caps = s.capabilities as u32;
        let new_caps = (old_caps & !input.unset) | input.set;
        if new_caps == old_caps {
            continue;
        }

        let mut active: server::ActiveModel = s.clone().into();
        active.capabilities = Set(new_caps as i32);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&txn).await?;
        count += 1;

        effects.push(CapabilityChangeEffect {
            server_id: s.id.clone(),
            old_caps,
            new_caps,
        });
    }
    txn.commit().await?;

    // Phase 2: Side effects (fire-and-forget, all idempotent)
    for effect in &effects {
        let CapabilityChangeEffect {
            server_id,
            old_caps,
            new_caps,
        } = effect;
        let new_caps = *new_caps;
        let old_caps = *old_caps;

        state.agent_manager.update_capabilities(server_id, new_caps);

        // Sync to agent if online and protocol v2+
        if let Some(pv) = state.agent_manager.get_protocol_version(server_id)
            && pv >= 2
            && let Some(tx) = state.agent_manager.get_sender(server_id)
        {
            let _ = tx
                .send(ServerMessage::CapabilitiesSync {
                    capabilities: new_caps,
                })
                .await;
        }

        // Broadcast to browsers
        state
            .agent_manager
            .broadcast_browser(BrowserMessage::CapabilitiesChanged {
                server_id: server_id.clone(),
                capabilities: new_caps,
            });

        // Re-sync ping tasks if ping bits changed
        let ping_mask = CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP;
        if old_caps & ping_mask != new_caps & ping_mask {
            PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, server_id).await;
        }

        // Docker capability revoked — teardown
        if has_capability(old_caps, CAP_DOCKER) && !has_capability(new_caps, CAP_DOCKER) {
            state.agent_manager.clear_docker_caches(server_id);
            state.docker_viewers.remove_all_for_server(server_id);
            let log_session_ids = state
                .agent_manager
                .remove_docker_log_sessions_for_server(server_id);
            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::DockerStopStats).await;
                let _ = tx.send(ServerMessage::DockerEventsStop).await;
                for sid in &log_session_ids {
                    let _ = tx
                        .send(ServerMessage::DockerLogsStop {
                            session_id: sid.clone(),
                        })
                        .await;
                }
            }
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
                    server_id: server_id.clone(),
                    available: false,
                });
        }

        // Audit log
        let detail = serde_json::json!({
            "server_id": server_id,
            "old": old_caps,
            "new": new_caps,
        })
        .to_string();
        let _ = AuditService::log(
            &state.db,
            user_id,
            "capabilities_changed",
            Some(&detail),
            &ip,
        )
        .await;
    }
```

- [ ] **Step 3: Add `use sea_orm::TransactionTrait`**

At the top of `server.rs`, ensure `TransactionTrait` is imported. Check existing imports; if `sea_orm` is already imported, add `TransactionTrait` to the use list. If not, add:

```rust
use sea_orm::TransactionTrait;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with 0 errors

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/api/server.rs
git commit -m "fix(server): wrap batch_update_capabilities in database transaction"
```

---

### Task 2: F2 — Security response headers

**Files:**
- Modify: `crates/server/Cargo.toml:21`
- Modify: `crates/server/src/router/mod.rs:16-34`

- [ ] **Step 1: Add `set-header` feature to tower-http**

In `crates/server/Cargo.toml`, change line 21 from:

```toml
tower-http = { version = "0.6", features = ["trace", "fs"] }
```

to:

```toml
tower-http = { version = "0.6", features = ["trace", "fs", "set-header"] }
```

- [ ] **Step 2: Add security headers to create_router**

In `crates/server/src/router/mod.rs`, add the import at the top:

```rust
use axum::http::HeaderValue;
use tower_http::set_header::SetResponseHeaderLayer;
```

Then replace the `create_router` function (lines 16-34) with:

```rust
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .nest("/api", api::router(state.clone()))
        // Agent WS: /api/agent/ws?token=<token> (no auth middleware, uses token param)
        .nest("/api", ws::agent::router())
        // Browser WS: /api/ws/servers (auth checked inside handler)
        .nest("/api", ws::browser::router())
        // Terminal WS: /api/ws/terminal/:server_id (auth checked inside handler)
        .nest("/api", ws::terminal::router())
        // Docker logs WS: /api/ws/docker/logs/:server_id (auth checked inside handler)
        .nest("/api", ws::docker_logs::router())
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Embedded frontend: serve static files, SPA fallback to index.html
        .fallback(static_files::static_handler)
        // Security headers
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::HeaderName::from_static("x-permitted-cross-domain-policies"),
            HeaderValue::from_static("none"),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with 0 errors

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 5: Add integration test for security headers**

In `crates/server/tests/integration.rs`, add a test at the end of the file:

```rust
#[tokio::test]
async fn test_security_headers_present() {
    let (base_url, _dir) = setup_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/healthz", base_url))
        .send()
        .await
        .expect("healthz request failed");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("x-frame-options").map(|v| v.to_str().unwrap()),
        Some("DENY"),
    );
    assert_eq!(
        resp.headers().get("x-content-type-options").map(|v| v.to_str().unwrap()),
        Some("nosniff"),
    );
    assert_eq!(
        resp.headers().get("referrer-policy").map(|v| v.to_str().unwrap()),
        Some("strict-origin-when-cross-origin"),
    );
    assert_eq!(
        resp.headers().get("x-permitted-cross-domain-policies").map(|v| v.to_str().unwrap()),
        Some("none"),
    );
}
```

- [ ] **Step 6: Run integration test**

Run: `cargo test -p serverbee-server --test integration test_security_headers_present`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/server/Cargo.toml crates/server/src/router/mod.rs crates/server/tests/integration.rs
git commit -m "feat(server): add security response headers (X-Frame-Options, X-Content-Type-Options, Referrer-Policy)"
```

---

### Task 3: F3 — File upload size limit

**Files:**
- Modify: `crates/server/src/config.rs:8-51`
- Modify: `crates/server/src/router/api/file.rs:119-128, 871-880`
- Modify: `ENV.md`

- [ ] **Step 1: Add FileConfig to config.rs**

In `crates/server/src/config.rs`, add the new struct after line 276 (after `UpgradeConfig`'s `Default` impl):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    /// Maximum file upload size in bytes. Default: 100 MB.
    #[serde(default = "default_max_upload_size")]
    pub max_upload_size: u64,
}

fn default_max_upload_size() -> u64 {
    104_857_600 // 100 MB
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            max_upload_size: default_max_upload_size(),
        }
    }
}
```

Then add the `file` field to `AppConfig` struct (after line 32, the `upgrade` field):

```rust
    #[serde(default)]
    pub file: FileConfig,
```

And in `AppConfig::default()` (inside the `Default` impl, after `upgrade: UpgradeConfig::default(),`):

```rust
            file: FileConfig::default(),
```

- [ ] **Step 2: Add size check to upload_file handler**

In `crates/server/src/router/api/file.rs`, find the chunk loop in `upload_file` (around line 876 where `file_size += chunk.len() as u64;`). Replace the accumulation line with:

```rust
                    file_size += chunk.len() as u64;
                    if file_size > state.config.file.max_upload_size {
                        let _ = tokio::fs::remove_file(&temp_upload).await;
                        return Err(AppError::BadRequest(format!(
                            "File size exceeds limit of {} bytes",
                            state.config.file.max_upload_size
                        )));
                    }
```

- [ ] **Step 3: Add DefaultBodyLimit to write_router**

In `crates/server/src/router/api/file.rs`, modify `write_router` to accept state for config access. Since `write_router` returns `Router<Arc<AppState>>` and the config is accessed at handler level, instead add `DefaultBodyLimit` as a layer on the upload route specifically.

Change `write_router()` (lines 119-128) to:

```rust
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/files/{server_id}/write", post(write_file))
        .route("/files/{server_id}/delete", post(delete_file))
        .route("/files/{server_id}/mkdir", post(mkdir))
        .route("/files/{server_id}/move", post(move_file))
        .route("/files/{server_id}/download", post(start_download))
        .route(
            "/files/{server_id}/upload",
            post(upload_file).layer(axum::extract::DefaultBodyLimit::max(110_100_480)), // 105 MB (100MB + 5MB multipart overhead)
        )
        .route("/files/transfers/{transfer_id}", delete(cancel_transfer))
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with 0 errors

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 6: Update ENV.md**

Add the following entry to `ENV.md` under the appropriate section:

```markdown
| `SERVERBEE_FILE__MAX_UPLOAD_SIZE` | `file.max_upload_size` | Maximum file upload size in bytes | `104857600` (100 MB) |
```

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/config.rs crates/server/src/router/api/file.rs ENV.md
git commit -m "feat(server): add file upload size limit with configurable max_upload_size"
```

---

### Task 4: F4 — /api/auth/me argon2 caching via session

**Files:**
- Create: `crates/server/src/migration/m20260328_000013_session_must_change_password.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Modify: `crates/server/src/entity/session.rs`
- Modify: `crates/server/src/middleware/auth.rs`
- Modify: `crates/server/src/service/auth.rs:88-139, 143-178`
- Modify: `crates/server/src/router/api/auth.rs:147-156, 247-271, 365-402`

- [ ] **Step 1: Create migration file**

Create `crates/server/src/migration/m20260328_000013_session_must_change_password.rs`:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260328_000013_session_must_change_password"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE sessions ADD COLUMN must_change_password BOOLEAN NOT NULL DEFAULT FALSE",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration**

In `crates/server/src/migration/mod.rs`, add the module import after line 14:

```rust
mod m20260328_000013_session_must_change_password;
```

And add to the `migrations()` vec after the last entry (line 32):

```rust
            Box::new(m20260328_000013_session_must_change_password::Migration),
```

- [ ] **Step 3: Add field to session entity**

In `crates/server/src/entity/session.rs`, add the new field to the `Model` struct after `created_at`:

```rust
    #[sea_orm(default_value = "false")]
    pub must_change_password: bool,
```

- [ ] **Step 4: Add must_change_password to CurrentUser**

In `crates/server/src/middleware/auth.rs`, update the `CurrentUser` struct (lines 14-18):

```rust
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub username: String,
    pub role: String,
    /// `Some(value)` for session auth, `None` for API key auth.
    pub must_change_password: Option<bool>,
}
```

Then update the session auth path in `auth_middleware` (lines 26-38). Change the `.map` closure to:

```rust
            .map(|(user, must_change_pw)| CurrentUser {
                user_id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
                must_change_password: Some(must_change_pw),
            })
```

And update the API key auth path (lines 44-53). Change the `.map` closure to:

```rust
                    .map(|user| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                        must_change_password: None,
                    })
```

- [ ] **Step 5: Update AuthService::validate_session return type**

In `crates/server/src/service/auth.rs`, change `validate_session` (lines 143-178) to return the `must_change_password` flag along with the user. Change the return type from `Result<Option<user::Model>, AppError>` to `Result<Option<(user::Model, bool)>, AppError>`.

Update the function body — change the final return (around line 175-177) from:

```rust
        let user = user::Entity::find_by_id(&user_id).one(db).await?;
        Ok(user)
```

to:

```rust
        let user = user::Entity::find_by_id(&user_id).one(db).await?;
        Ok(user.map(|u| (u, must_change_pw)))
```

And extract `must_change_pw` from the session model. After line 168 (`let user_id = session.user_id.clone();`), add:

```rust
        let must_change_pw = session.must_change_password;
```

- [ ] **Step 6: Update AuthService::login_with_totp to accept must_change_password**

In `crates/server/src/service/auth.rs`, modify `login_with_totp` (lines 88-139):

Add `must_change_password: bool` parameter after `session_ttl: i64`:

```rust
    pub async fn login_with_totp(
        db: &DatabaseConnection,
        username: &str,
        password: &str,
        totp_code: Option<&str>,
        ip: &str,
        user_agent: &str,
        session_ttl: i64,
        must_change_password: bool,
    ) -> Result<(session::Model, user::Model), AppError> {
```

Update the session creation (lines 127-135) to include the new field:

```rust
        let new_session = session::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user.id.clone()),
            token: Set(token),
            ip: Set(ip.to_string()),
            user_agent: Set(user_agent.to_string()),
            expires_at: Set(expires_at),
            created_at: Set(now),
            must_change_password: Set(must_change_password),
        };
```

Also update the `login` convenience method (around line 84) to pass `false`:

```rust
    pub async fn login(
        db: &DatabaseConnection,
        username: &str,
        password: &str,
        ip: &str,
        user_agent: &str,
        session_ttl: i64,
    ) -> Result<(session::Model, user::Model), AppError> {
        Self::login_with_totp(db, username, password, None, ip, user_agent, session_ttl, false).await
    }
```

- [ ] **Step 7: Update login handler to pass must_change_password**

In `crates/server/src/router/api/auth.rs`, update the login handler (around lines 147-156). The `must_change_password` is already computed at line 187 via `body.password == "admin" && user.username == "admin"`, but it's computed AFTER `login_with_totp` is called. Restructure:

Compute `must_change_password` BEFORE calling `login_with_totp`, then pass it. Replace lines 147-187 with:

```rust
    let must_change_password = body.password == "admin" && body.username == "admin";

    let (session, user) = AuthService::login_with_totp(
        &state.db,
        &body.username,
        &body.password,
        body.totp_code.as_deref(),
        &ip,
        &user_agent,
        state.config.auth.session_ttl,
        must_change_password,
    )
    .await?;
```

Remove the old line 187 (`let must_change_password = body.password == "admin" && user.username == "admin";`) since it's now computed above.

- [ ] **Step 8: Update /me handler to read from CurrentUser**

In `crates/server/src/router/api/auth.rs`, replace the `me` handler (lines 247-271) with:

```rust
pub async fn me(
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<MeResponse>>, AppError> {
    ok(MeResponse {
        user_id: current_user.user_id,
        username: current_user.username,
        role: current_user.role,
        must_change_password: current_user.must_change_password.unwrap_or(false),
    })
}
```

Remove the `State(state): State<Arc<AppState>>` parameter since we no longer need DB access.

- [ ] **Step 9: Clear flag on password change**

In `crates/server/src/router/api/auth.rs`, after the `AuthService::change_password` call in `change_password` handler (after line 384), add:

```rust
    // Clear must_change_password flag on all sessions for this user
    session::Entity::update_many()
        .col_expr(
            session::Column::MustChangePassword,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(session::Column::UserId.eq(&current_user.user_id))
        .exec(&state.db)
        .await?;
```

Add `use crate::entity::session;` at the top of the file if not already imported.

- [ ] **Step 10: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with 0 errors

- [ ] **Step 11: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass. Existing auth tests should still work since `must_change_password` defaults to `false`.

- [ ] **Step 12: Commit**

```bash
git add crates/server/src/migration/ crates/server/src/entity/session.rs crates/server/src/middleware/auth.rs crates/server/src/service/auth.rs crates/server/src/router/api/auth.rs
git commit -m "perf(server): cache must_change_password in session, eliminate argon2 from /me"
```

---

### Task 5: F5 — fetch_external_ip response size limit

**Files:**
- Modify: `crates/agent/src/reporter.rs:1575-1583`

- [ ] **Step 1: Add response size limit**

In `crates/agent/src/reporter.rs`, replace `fetch_external_ip` (lines 1575-1583) with:

```rust
async fn fetch_external_ip(url: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let resp = client.get(url).send().await?;

    // Reject responses larger than 256 bytes to prevent memory exhaustion
    if let Some(len) = resp.content_length() {
        if len > 256 {
            anyhow::bail!("External IP response too large: {len} bytes");
        }
    }

    let bytes = resp.bytes().await?;
    if bytes.len() > 256 {
        anyhow::bail!("External IP response too large: {} bytes", bytes.len());
    }

    let ip = String::from_utf8_lossy(&bytes).trim().to_string();
    Ok(ip)
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles with 0 errors

- [ ] **Step 3: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "fix(agent): limit fetch_external_ip response to 256 bytes"
```

---

### Task 6: Final verification and clippy

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 0 warnings

- [ ] **Step 3: Run frontend checks**

Run: `cd apps/web && bun run typecheck && bun x ultracite check`
Expected: no errors (no frontend changes in this plan, but verify nothing broke)

- [ ] **Step 4: Update PROGRESS.md**

Add a new section to `docs/superpowers/plans/PROGRESS.md` documenting the security hardening round 2 completion with task list and commit hashes.

- [ ] **Step 5: Commit progress update**

```bash
git add docs/superpowers/plans/PROGRESS.md
git commit -m "docs: update progress for security hardening round 2"
```
