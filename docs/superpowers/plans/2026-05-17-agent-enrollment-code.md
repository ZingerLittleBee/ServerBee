# Agent Enrollment Code Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ServerBee's permanent, plaintext, globally-shared `auto_discovery_key` with one-time, short-TTL, argon2-hashed enrollment codes (ported from forward-rs/portunus' enrollment model), and add run-token revoke/rotate.

**Architecture:** A new `agent_enrollments` table stores argon2 hashes of single-use enrollment codes with an expiry. Admins mint a code (plaintext returned exactly once); an agent presents it as a Bearer token to `POST /api/agent/register`, which verifies it in constant time, consumes it atomically, then issues the existing per-server run token (unchanged). The old discovery-key path is removed entirely. A new admin endpoint can revoke/rotate a server's run token and force the agent to disconnect.

**Tech Stack:** Rust (Axum 0.8, sea-orm 1.x, SQLite, argon2 via existing `AuthService`), React 19 frontend (TanStack Router/Query, shadcn/ui).

---

## Backward-Compatibility Decision (resolved — confirm before executing)

**Decision: do NOT keep the old `auto_discovery_key` path.** Rationale:

- Existing **deployed** agents already hold a persisted per-server run token in `agent.toml`. The run-token auth path (`validate_agent_token`, argon2, prefix lookup) is **unchanged**, so already-registered agents keep working without interruption. Only the *first-time onboarding* path changes.
- The only thing dropping back-compat breaks is onboarding a brand-new agent using the old shared key — which is exactly the takeover vector being removed. Keeping it as a fallback would preserve the vulnerability and double the surface (the project's CLAUDE.md explicitly disallows back-compat shims "when you can just change the code").
- Migration deletes the `auto_discovery_key` config row so no stale secret lingers in backups.

If the user rejects this, the only change is: in Task 4 keep the old key branch as a fallback inside `register` (adds ~20 lines + a config row). Default plan assumes removal.

---

## File Structure

- `crates/server/src/migration/m20260517_000022_create_agent_enrollment.rs` — new table + delete stale `auto_discovery_key` config row. One responsibility: schema delta.
- `crates/server/src/migration/mod.rs` — register the migration.
- `crates/server/src/entity/agent_enrollment.rs` — sea-orm entity for the new table.
- `crates/server/src/entity/mod.rs` — register the entity module.
- `crates/server/src/service/enrollment.rs` — all enrollment business logic (mint / verify+consume / list / delete / prune). One responsibility: enrollment lifecycle.
- `crates/server/src/service/mod.rs` — register the service module.
- `crates/server/src/router/api/agent.rs` — rewrite `register` to consume an enrollment code; add admin enrollment CRUD endpoints; add run-token revoke endpoint.
- `crates/server/src/router/api/setting.rs` — remove the two auto-discovery-key endpoints.
- `crates/server/src/main.rs` — remove `init_auto_discovery_key` and its credentials line.
- `crates/agent/src/config.rs` — rename `auto_discovery_key` field to `enrollment_code`.
- `crates/agent/src/register.rs` — surface server error body; return/log `server_id`.
- `crates/agent/src/main.rs` — update field name + log messages.
- `apps/web/src/routes/_authed/settings/index.tsx` — replace the discovery-key card with enrollment-code generation + copyable install command.
- `apps/web/src/lib/api-client.ts` (or wherever API calls live) — add enrollment API calls.
- `ENV.md`, `apps/docs/content/docs/{en,cn}/configuration.mdx`, `apps/docs/content/docs/{cn,en}/agent.mdx` — replace `auto_discovery_key` docs with enrollment-code flow.

---

## Conventions to follow (from the existing codebase)

- Enrollment code = `AuthService::generate_session_token()` (32 bytes OsRng → base64url, 43 chars, ~256-bit).
- Store `code_hash = AuthService::hash_password(code)` (argon2) and `code_prefix = &code[..8]`. Look up by prefix, verify with `AuthService::verify_password` (argon2 verify is constant-time). This mirrors `validate_api_key` / `validate_agent_token` exactly.
- Migrations: implement `up()` only; leave `down()` as `Ok(())`.
- All endpoints return `Json<ApiResponse<T>>` via `ok(...)`; DTOs derive `utoipa::ToSchema`; every endpoint annotated with `#[utoipa::path]`.
- Admin-only write endpoints go on the router guarded by `require_admin` (find where `users`/`api-keys` admin routes are mounted and follow the same pattern).
- Audit sensitive actions via `AuditService::log(&db, user_id, action, detail, ip)`.

---

### Task 1: Migration + entity for `agent_enrollments`

**Files:**
- Create: `crates/server/src/migration/m20260517_000022_create_agent_enrollment.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Create: `crates/server/src/entity/agent_enrollment.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Write the migration file**

Create `crates/server/src/migration/m20260517_000022_create_agent_enrollment.rs`:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260517_000022_create_agent_enrollment"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS agent_enrollments (
                id TEXT PRIMARY KEY NOT NULL,
                code_hash TEXT NOT NULL,
                code_prefix TEXT NOT NULL,
                label TEXT,
                created_by TEXT NOT NULL,
                expires_at DATETIME NOT NULL,
                consumed_at DATETIME,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_agent_enrollments_code_prefix
                ON agent_enrollments(code_prefix)",
        )
        .await?;
        // Remove the now-obsolete permanent shared discovery key so it
        // does not linger in DB backups.
        db.execute_unprepared(
            "DELETE FROM config WHERE key = 'auto_discovery_key'",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register the migration**

In `crates/server/src/migration/mod.rs`, add the `mod` line after the last existing `mod m20260430_000021...;` line:

```rust
mod m20260517_000022_create_agent_enrollment;
```

And add to the `migrations()` vec, after the `m20260430_000021_custom_theme_ref_integrity::Migration` entry:

```rust
            Box::new(m20260517_000022_create_agent_enrollment::Migration),
```

- [ ] **Step 3: Write the entity**

Create `crates/server/src/entity/agent_enrollment.rs`:

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_enrollments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code_hash: String,
    pub code_prefix: String,
    pub label: Option<String>,
    pub created_by: String,
    pub expires_at: DateTimeUtc,
    pub consumed_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 4: Register the entity module**

In `crates/server/src/entity/mod.rs`, add (keep the list alphabetically ordered — insert before `pub mod alert_rule;`):

```rust
pub mod agent_enrollment;
```

- [ ] **Step 5: Verify it compiles and migrations run**

Run: `cargo build -p serverbee-server`
Expected: builds with no errors.

Run: `cargo test -p serverbee-server --lib config::tests::test_get_set_config`
Expected: PASS (confirms `setup_test_db` still applies all migrations including the new one).

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/migration/m20260517_000022_create_agent_enrollment.rs crates/server/src/migration/mod.rs crates/server/src/entity/agent_enrollment.rs crates/server/src/entity/mod.rs
git commit -m "feat(server): add agent_enrollments table and entity"
```

---

### Task 2: EnrollmentService (mint / verify+consume / list / delete / prune)

**Files:**
- Create: `crates/server/src/service/enrollment.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/server/src/service/enrollment.rs` with only the test module first:

```rust
use chrono::{Duration, Utc};
use sea_orm::*;
use uuid::Uuid;

use crate::entity::agent_enrollment;
use crate::error::AppError;
use crate::service::auth::AuthService;

pub const DEFAULT_TTL_SECS: i64 = 600;
pub const MAX_TTL_SECS: i64 = 86_400;

pub struct EnrollmentService;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn mint_returns_plaintext_and_stores_hash() {
        let (db, _tmp) = setup_test_db().await;
        let (model, code) = EnrollmentService::mint(&db, "admin-1", None, 600)
            .await
            .expect("mint");
        assert_eq!(code.len(), 43, "code is a 43-char base64url token");
        assert_ne!(model.code_hash, code, "hash must not equal plaintext");
        assert!(model.code_hash.starts_with("$argon2"));
        assert_eq!(model.code_prefix, &code[..8]);
        assert!(model.consumed_at.is_none());
    }

    #[tokio::test]
    async fn mint_rejects_bad_ttl() {
        let (db, _tmp) = setup_test_db().await;
        assert!(EnrollmentService::mint(&db, "a", None, 0).await.is_err());
        assert!(
            EnrollmentService::mint(&db, "a", None, MAX_TTL_SECS + 1)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn verify_and_consume_succeeds_once() {
        let (db, _tmp) = setup_test_db().await;
        let (_m, code) = EnrollmentService::mint(&db, "admin-1", None, 600)
            .await
            .expect("mint");

        let first = EnrollmentService::verify_and_consume(&db, &code)
            .await
            .expect("verify ok");
        assert!(first.is_some(), "first redemption succeeds");

        let second = EnrollmentService::verify_and_consume(&db, &code)
            .await
            .expect("verify ok");
        assert!(second.is_none(), "second redemption rejected (single-use)");
    }

    #[tokio::test]
    async fn verify_rejects_expired() {
        let (db, _tmp) = setup_test_db().await;
        let (_m, code) = EnrollmentService::mint(&db, "admin-1", None, 600)
            .await
            .expect("mint");
        // Force expiry into the past.
        agent_enrollment::Entity::update_many()
            .col_expr(
                agent_enrollment::Column::ExpiresAt,
                sea_orm::sea_query::Expr::value(Utc::now() - Duration::seconds(10)),
            )
            .exec(&db)
            .await
            .expect("expire");

        let r = EnrollmentService::verify_and_consume(&db, &code)
            .await
            .expect("verify ok");
        assert!(r.is_none(), "expired code rejected");
    }

    #[tokio::test]
    async fn verify_rejects_unknown_code() {
        let (db, _tmp) = setup_test_db().await;
        let r = EnrollmentService::verify_and_consume(&db, "totally-wrong-code-value-xyz")
            .await
            .expect("verify ok");
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn prune_removes_expired_and_consumed() {
        let (db, _tmp) = setup_test_db().await;
        let (_m, code) = EnrollmentService::mint(&db, "a", None, 600).await.unwrap();
        EnrollmentService::verify_and_consume(&db, &code)
            .await
            .unwrap();
        let removed = EnrollmentService::prune(&db).await.expect("prune");
        assert_eq!(removed, 1, "consumed enrollment pruned");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server --lib service::enrollment`
Expected: FAIL — `EnrollmentService::mint` / `verify_and_consume` / `prune` not found.

(If the module isn't picked up yet, add `pub mod enrollment;` to `crates/server/src/service/mod.rs` now — keep alphabetical order, e.g. before `pub mod network_probe;`.)

- [ ] **Step 3: Implement the service**

Insert the implementation in `crates/server/src/service/enrollment.rs` directly above `#[cfg(test)] mod tests`:

```rust
impl EnrollmentService {
    /// Mint a new single-use enrollment code. Returns the stored model and
    /// the plaintext code (shown to the operator exactly once).
    pub async fn mint(
        db: &DatabaseConnection,
        created_by: &str,
        label: Option<String>,
        ttl_secs: i64,
    ) -> Result<(agent_enrollment::Model, String), AppError> {
        if ttl_secs <= 0 || ttl_secs > MAX_TTL_SECS {
            return Err(AppError::BadRequest(format!(
                "ttl_secs must be between 1 and {MAX_TTL_SECS}"
            )));
        }
        let code = AuthService::generate_session_token();
        let code_hash = AuthService::hash_password(&code)?;
        let code_prefix = code[..8.min(code.len())].to_string();
        let now = Utc::now();

        let model = agent_enrollment::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            code_hash: Set(code_hash),
            code_prefix: Set(code_prefix),
            label: Set(label),
            created_by: Set(created_by.to_string()),
            expires_at: Set(now + Duration::seconds(ttl_secs)),
            consumed_at: Set(None),
            created_at: Set(now),
        }
        .insert(db)
        .await?;

        Ok((model, code))
    }

    /// Verify a presented code and atomically consume it. Returns the
    /// enrollment row on success, `None` if unknown / expired / already used.
    pub async fn verify_and_consume(
        db: &DatabaseConnection,
        code: &str,
    ) -> Result<Option<agent_enrollment::Model>, AppError> {
        if code.len() < 8 {
            return Ok(None);
        }
        let prefix = &code[..8];
        let candidates = agent_enrollment::Entity::find()
            .filter(agent_enrollment::Column::CodePrefix.eq(prefix))
            .all(db)
            .await?;

        let now = Utc::now();
        for candidate in candidates {
            // argon2 verify is constant-time; run it before the
            // expiry/consumed checks so timing does not reveal which
            // branch rejected the code.
            if AuthService::verify_password(code, &candidate.code_hash)? {
                if candidate.consumed_at.is_some() || candidate.expires_at < now {
                    return Ok(None);
                }
                // Atomic single-use: only the row still NULL on consumed_at
                // is updated. A concurrent redeemer loses the race.
                let res = agent_enrollment::Entity::update_many()
                    .col_expr(
                        agent_enrollment::Column::ConsumedAt,
                        sea_orm::sea_query::Expr::value(now),
                    )
                    .filter(agent_enrollment::Column::Id.eq(&candidate.id))
                    .filter(agent_enrollment::Column::ConsumedAt.is_null())
                    .exec(db)
                    .await?;
                if res.rows_affected == 0 {
                    return Ok(None);
                }
                return Ok(Some(candidate));
            }
        }
        Ok(None)
    }

    pub async fn list(
        db: &DatabaseConnection,
    ) -> Result<Vec<agent_enrollment::Model>, AppError> {
        Ok(agent_enrollment::Entity::find()
            .order_by_desc(agent_enrollment::Column::CreatedAt)
            .all(db)
            .await?)
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let res = agent_enrollment::Entity::delete_by_id(id).exec(db).await?;
        if res.rows_affected == 0 {
            return Err(AppError::NotFound("Enrollment not found".to_string()));
        }
        Ok(())
    }

    /// Delete expired or consumed enrollments. Returns the number removed.
    pub async fn prune(db: &DatabaseConnection) -> Result<u64, AppError> {
        let now = Utc::now();
        let res = agent_enrollment::Entity::delete_many()
            .filter(
                agent_enrollment::Column::ConsumedAt
                    .is_not_null()
                    .or(agent_enrollment::Column::ExpiresAt.lt(now)),
            )
            .exec(db)
            .await?;
        Ok(res.rows_affected)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server --lib service::enrollment`
Expected: all 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/enrollment.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add EnrollmentService for single-use TTL agent codes"
```

---

### Task 3: Admin endpoints — create / list / delete enrollment

**Files:**
- Modify: `crates/server/src/router/api/agent.rs`

- [ ] **Step 1: Add request/response DTOs**

In `crates/server/src/router/api/agent.rs`, add near the existing `RegisterResponse` struct:

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateEnrollmentRequest {
    #[serde(default)]
    label: Option<String>,
    /// Lifetime in seconds. Defaults to 600 (10 minutes), max 86400.
    ttl_secs: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateEnrollmentResponse {
    id: String,
    /// Plaintext enrollment code — shown exactly once, never retrievable again.
    code: String,
    expires_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnrollmentSummary {
    id: String,
    label: Option<String>,
    code_prefix: String,
    created_by: String,
    expires_at: String,
    consumed_at: Option<String>,
    created_at: String,
}
```

- [ ] **Step 2: Add the admin router and handlers**

Add to `crates/server/src/router/api/agent.rs`. Add imports at the top: `use axum::extract::Path;` and `use crate::service::enrollment::{EnrollmentService, DEFAULT_TTL_SECS};` and `use crate::service::audit::AuditService;` and `use crate::middleware::auth::AuthUser;` (verify the exact extractor used by other admin handlers — e.g. `api_key.rs` — and mirror it; the snippet below assumes an `AuthUser` extractor exposing `.id` and `.ip`; adjust to the real type).

```rust
/// Admin-only routes for managing enrollment codes.
pub fn admin_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agent/enrollments", post(create_enrollment))
        .route("/agent/enrollments", get(list_enrollments))
        .route("/agent/enrollments/{id}", axum::routing::delete(delete_enrollment))
}

#[utoipa::path(
    post,
    path = "/api/agent/enrollments",
    tag = "agent",
    request_body = CreateEnrollmentRequest,
    responses((status = 200, description = "Enrollment code created", body = CreateEnrollmentResponse)),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_enrollment(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<CreateEnrollmentRequest>,
) -> Result<Json<ApiResponse<CreateEnrollmentResponse>>, AppError> {
    let ttl = body.ttl_secs.unwrap_or(DEFAULT_TTL_SECS);
    let (model, code) =
        EnrollmentService::mint(&state.db, &user.id, body.label.clone(), ttl).await?;
    AuditService::log(
        &state.db,
        &user.id,
        "agent_enrollment_created",
        Some(&format!("id={} prefix={}", model.id, model.code_prefix)),
        &user.ip,
    )
    .await?;
    ok(CreateEnrollmentResponse {
        id: model.id,
        code,
        expires_at: model.expires_at.to_rfc3339(),
    })
}

#[utoipa::path(
    get,
    path = "/api/agent/enrollments",
    tag = "agent",
    responses((status = 200, description = "List enrollment codes", body = [EnrollmentSummary])),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_enrollments(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<EnrollmentSummary>>>, AppError> {
    let rows = EnrollmentService::list(&state.db).await?;
    let out = rows
        .into_iter()
        .map(|m| EnrollmentSummary {
            id: m.id,
            label: m.label,
            code_prefix: m.code_prefix,
            created_by: m.created_by,
            expires_at: m.expires_at.to_rfc3339(),
            consumed_at: m.consumed_at.map(|d| d.to_rfc3339()),
            created_at: m.created_at.to_rfc3339(),
        })
        .collect();
    ok(out)
}

#[utoipa::path(
    delete,
    path = "/api/agent/enrollments/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Enrollment id")),
    responses((status = 200, description = "Deleted")),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_enrollment(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    EnrollmentService::delete(&state.db, &id).await?;
    AuditService::log(
        &state.db,
        &user.id,
        "agent_enrollment_deleted",
        Some(&format!("id={id}")),
        &user.ip,
    )
    .await?;
    ok("deleted")
}
```

- [ ] **Step 3: Mount the admin router under require_admin**

Find where admin API sub-routers are mounted (search: `grep -rn "require_admin\|admin_router\|api_key::router" crates/server/src/router`). Mount `agent::admin_router()` on the same admin-guarded layer that `users`/`api-keys` use, following the identical pattern. Adjust `AuthUser` extractor name/fields to match what those handlers actually use.

- [ ] **Step 4: Build and add an integration test**

Add to `crates/server/src/router/api/agent.rs` test module (create one if absent, mirroring another `router/api` module's tests) a test that mints then lists:

```rust
#[cfg(test)]
mod enrollment_endpoint_tests {
    use crate::service::enrollment::EnrollmentService;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn mint_then_list_shows_prefix_not_code() {
        let (db, _tmp) = setup_test_db().await;
        let (_m, code) = EnrollmentService::mint(&db, "admin-1", None, 600)
            .await
            .unwrap();
        let list = EnrollmentService::list(&db).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code_prefix, &code[..8]);
        assert!(
            !list[0].code_hash.contains(&code),
            "plaintext code never stored"
        );
    }
}
```

Run: `cargo test -p serverbee-server --lib agent`
Expected: PASS. Also run `cargo build -p serverbee-server` — no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/agent.rs crates/server/src/router/mod.rs
git commit -m "feat(server): admin endpoints to mint/list/delete agent enrollment codes"
```

---

### Task 4: Rewrite `register` to consume an enrollment code

**Files:**
- Modify: `crates/server/src/router/api/agent.rs` (the `register` handler, lines ~95–166)

- [ ] **Step 1: Replace the discovery-key validation block**

In `register`, delete the block currently spanning the `stored_key` lookup and the `if auth_header != stored_key` check (the lines that read `CONFIG_KEY_AUTO_DISCOVERY` from `ConfigService` and compare). Replace with enrollment-code verification:

```rust
    // 2. Enrollment code validation (single-use, TTL, constant-time argon2)
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let enrollment =
        crate::service::enrollment::EnrollmentService::verify_and_consume(&state.db, auth_header)
            .await?
            .ok_or(AppError::Unauthorized)?;
```

Also delete the now-unused `const CONFIG_KEY_AUTO_DISCOVERY` at the top of the file and the `use crate::service::config::ConfigService;` import **if** nothing else in this file uses it (check first with a grep within the file).

- [ ] **Step 2: Audit successful redemption and token issuance**

Just before each of the two `return ok(RegisterResponse { ... })` points and the final `ok(RegisterResponse { ... })`, the server has `server_id`, `ip`, `enrollment.id`, `enrollment.code_prefix`. Add (once, right after a successful server create/reuse, before returning) — extract a small closure or repeat the call at each return path:

```rust
    crate::service::audit::AuditService::log(
        &state.db,
        "system",
        "agent_enrolled",
        Some(&format!(
            "server_id={server_id} enrollment={} prefix={}",
            enrollment.id, enrollment.code_prefix
        )),
        &ip,
    )
    .await
    .ok();
```

Place this immediately before each `return ok(RegisterResponse {` and before the final `ok(RegisterResponse {` so all three exit paths are covered. (Repeating the 10 lines at three sites is acceptable here; do not introduce an abstraction.)

- [ ] **Step 3: Update the OpenAPI doc comment**

Change the `#[utoipa::path]` `responses` for `register`: replace the `(status = 401, description = "Invalid auto-discovery key")` line with `(status = 401, description = "Invalid, expired, or already-used enrollment code")`. Remove the `(status = 400, description = "Auto-discovery key not configured ...")` line (keep the server-limit 400).

- [ ] **Step 4: Add an integration test for the new register flow**

In the `agent.rs` test module add (uses `EnrollmentService` + `validate_agent_token` round-trip; if a full HTTP test harness exists in `tests/`, prefer that — otherwise this service-level test guards the contract):

```rust
#[tokio::test]
async fn register_consumes_code_then_rejects_reuse() {
    use crate::service::auth::AuthService;
    use crate::service::enrollment::EnrollmentService;
    use crate::test_utils::setup_test_db;

    let (db, _tmp) = setup_test_db().await;
    let (_m, code) = EnrollmentService::mint(&db, "admin", None, 600)
        .await
        .unwrap();

    // Simulate register: verify+consume must succeed once.
    assert!(
        EnrollmentService::verify_and_consume(&db, &code)
            .await
            .unwrap()
            .is_some()
    );
    // A second register attempt with the same code must fail.
    assert!(
        EnrollmentService::verify_and_consume(&db, &code)
            .await
            .unwrap()
            .is_none()
    );
    // Sanity: token machinery still intact.
    let t = AuthService::generate_session_token();
    assert_eq!(t.len(), 43);
}
```

Run: `cargo test -p serverbee-server --lib agent`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/agent.rs
git commit -m "feat(server): register agents via single-use enrollment code instead of shared key"
```

---

### Task 5: Run-token revoke/rotate endpoint + force disconnect

**Files:**
- Modify: `crates/server/src/router/api/agent.rs`

- [ ] **Step 1: Inspect AgentManager disconnect API**

Run: `grep -rn "fn disconnect\|fn remove\|fn kick\|pub fn .*agent" crates/server/src/state.rs crates/server/src/router/ws/agent.rs`
Read the result. Identify the method that drops a connected agent's WS sender by `server_id` (e.g. `state.agent_manager.disconnect(&server_id)`). Use the real method name in Step 2.

- [ ] **Step 2: Add the rotate endpoint**

Add to `crates/server/src/router/api/agent.rs`:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RotateTokenResponse {
    server_id: String,
    /// New plaintext run token — shown once. The agent must be re-enrolled
    /// or reconfigured with this value (or it will re-register).
    token: String,
}

#[utoipa::path(
    post,
    path = "/api/agent/{id}/rotate-token",
    tag = "agent",
    params(("id" = String, Path, description = "Server id")),
    responses(
        (status = 200, description = "Token rotated; old token revoked", body = RotateTokenResponse),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn rotate_token(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<RotateTokenResponse>>, AppError> {
    let existing = server::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;

    let plaintext = AuthService::generate_session_token();
    let token_hash = AuthService::hash_password(&plaintext)?;
    let token_prefix = plaintext[..8.min(plaintext.len())].to_string();

    let mut active: server::ActiveModel = existing.into();
    active.token_hash = Set(token_hash);
    active.token_prefix = Set(token_prefix);
    active.updated_at = Set(Utc::now());
    active.update(&state.db).await?;

    // Force the currently-connected agent (if any) to drop; its old token
    // no longer validates so it will reconnect/re-register.
    // Replace `disconnect` with the real AgentManager method from Step 1.
    state.agent_manager.disconnect(&id);

    AuditService::log(
        &state.db,
        &user.id,
        "agent_token_rotated",
        Some(&format!("server_id={id}")),
        &user.ip,
    )
    .await?;

    ok(RotateTokenResponse {
        server_id: id,
        token: plaintext,
    })
}
```

Add `.route("/agent/{id}/rotate-token", post(rotate_token))` to `admin_router()`.

- [ ] **Step 3: Build + test**

Run: `cargo build -p serverbee-server`
Expected: compiles (after substituting the real disconnect method name).

Run: `cargo clippy -p serverbee-server -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/agent.rs
git commit -m "feat(server): admin endpoint to rotate/revoke a server run token"
```

---

### Task 6: Periodic prune of expired/consumed enrollments

**Files:**
- Modify: `crates/server/src/task/cleanup.rs`

- [ ] **Step 1: Inspect the cleanup task**

Run: `grep -n "async fn run\|interval\|delete\|prune\|ConfigService" crates/server/src/task/cleanup.rs | head`
Read the loop structure to match its style (interval, error handling).

- [ ] **Step 2: Add the prune call**

Inside the cleanup task's periodic loop body, alongside the other cleanup calls, add:

```rust
    match crate::service::enrollment::EnrollmentService::prune(&state.db).await {
        Ok(n) if n > 0 => tracing::info!("Pruned {n} expired/consumed enrollments"),
        Ok(_) => {}
        Err(e) => tracing::warn!("Enrollment prune failed: {e}"),
    }
```

(Match the exact `state`/`db` binding name the existing cleanup body uses.)

- [ ] **Step 3: Build**

Run: `cargo build -p serverbee-server`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/cleanup.rs
git commit -m "feat(server): prune expired/consumed enrollments in cleanup task"
```

---

### Task 7: Remove `auto_discovery_key` from server

**Files:**
- Modify: `crates/server/src/router/api/setting.rs`
- Modify: `crates/server/src/main.rs`
- Modify: `crates/server/src/config.rs` (the `auth.auto_discovery_key` field)

- [ ] **Step 1: Remove setting endpoints**

In `crates/server/src/router/api/setting.rs`:
- Delete the two routes `.route("/settings/auto-discovery-key", get(get_auto_discovery_key))` and `.route("/settings/auto-discovery-key", put(regenerate_auto_discovery_key))`.
- Delete the `get_auto_discovery_key` and `regenerate_auto_discovery_key` functions, the `AutoDiscoveryKeyResponse` struct, and the `const CONFIG_KEY_AUTO_DISCOVERY`.
- Remove the now-unused `use crate::service::auth::AuthService;` only if nothing else in the file uses it (grep within file first).

- [ ] **Step 2: Remove init from main.rs**

In `crates/server/src/main.rs`:
- Delete the `let auto_discovery_key = init_auto_discovery_key(&db, &config).await?;` line.
- Delete the entire `async fn init_auto_discovery_key(...)` function.
- In the credentials block, remove the line `credentials.push_str(&format!("\n  Auto-discovery key:  {}", auto_discovery_key));` and change the guard `if generated_admin_password.is_some() || !auto_discovery_key.is_empty()` to `if let Some(ref pwd) = generated_admin_password {` wrapping just the admin lines (so the block only prints when an admin password was generated). Remove any now-unused imports (`URL_SAFE_NO_PAD`, `rand::rngs::OsRng`, `ConfigService`) flagged by the compiler.

- [ ] **Step 3: Remove the config field**

Run: `grep -n "auto_discovery_key" crates/server/src/config.rs`
Remove the `auto_discovery_key` field from the `AuthConfig` (or wherever `auth.auto_discovery_key` is defined) and any default. Run `grep -rn "auto_discovery_key" crates/server/` to confirm zero remaining references.

- [ ] **Step 4: Build + full server test suite**

Run: `cargo build -p serverbee-server && cargo clippy -p serverbee-server -- -D warnings`
Expected: 0 errors, 0 warnings.

Run: `cargo test -p serverbee-server`
Expected: all tests PASS (existing register tests that referenced the old key must have been updated in Task 4; if any remain, fix them to use `EnrollmentService::mint`).

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/setting.rs crates/server/src/main.rs crates/server/src/config.rs
git commit -m "refactor(server): remove obsolete auto_discovery_key shared secret"
```

---

### Task 8: Agent side — enrollment code config + better errors

**Files:**
- Modify: `crates/agent/src/config.rs`
- Modify: `crates/agent/src/register.rs`
- Modify: `crates/agent/src/main.rs`

- [ ] **Step 1: Rename the config field**

In `crates/agent/src/config.rs`, change:

```rust
    #[serde(default)]
    pub auto_discovery_key: String,
```

to:

```rust
    #[serde(default)]
    pub enrollment_code: String,
```

- [ ] **Step 2: Surface server error body in register**

In `crates/agent/src/register.rs`, replace the body of `register_agent`'s error handling. Change:

```rust
    if !resp.status().is_success() {
        anyhow::bail!("Registration failed: HTTP {}", resp.status());
    }
    let data: RegisterResponse = resp.json().await?;
    Ok((data.data.server_id, data.data.token))
```

to:

```rust
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Registration failed: HTTP {status}. Server said: {}. \
             Check that the enrollment code is valid and not expired/used.",
            body.trim()
        );
    }
    let data: RegisterResponse = resp.json().await?;
    tracing::info!("Registered as server_id={}", data.data.server_id);
    Ok((data.data.server_id, data.data.token))
```

And change `.bearer_auth(&config.auto_discovery_key)` to `.bearer_auth(&config.enrollment_code)`.

- [ ] **Step 3: Update main.rs**

In `crates/agent/src/main.rs`, change:

```rust
        if config.auto_discovery_key.is_empty() {
            anyhow::bail!("No token and no auto_discovery_key. Set one in config.");
        }
```

to:

```rust
        if config.enrollment_code.is_empty() {
            anyhow::bail!(
                "No token and no enrollment_code. Generate a code in the \
                 server UI (Settings → Agents) and set `enrollment_code` in \
                 agent.toml or SERVERBEE_ENROLLMENT_CODE."
            );
        }
```

Change `let (_server_id, token) = register::register_agent(...)` to `let (server_id, token) = register::register_agent(...)` and add `tracing::info!("Agent bound to server_id={server_id}");` after `save_token`.

- [ ] **Step 4: Build + agent tests**

Run: `cargo build -p serverbee-agent && cargo test -p serverbee-agent && cargo clippy -p serverbee-agent -- -D warnings`
Expected: PASS, 0 warnings. (Tests in `register.rs`/`config.rs` don't reference the renamed field; if any do, update them.)

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/config.rs crates/agent/src/register.rs crates/agent/src/main.rs
git commit -m "feat(agent): use one-time enrollment_code and surface server registration errors"
```

---

### Task 9: Docs / ENV / install script

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`, `apps/docs/content/docs/cn/configuration.mdx`
- Modify: `apps/docs/content/docs/en/agent.mdx`, `apps/docs/content/docs/cn/agent.mdx`
- Modify: `deploy/install.sh` (the agent install flag)

- [ ] **Step 1: Find every doc mention**

Run: `grep -rn "auto_discovery_key\|auto-discovery\|AUTO_DISCOVERY\|SERVERBEE_AUTO" ENV.md apps/docs/content deploy/`
This is the authoritative list of spots to rewrite.

- [ ] **Step 2: Rewrite each occurrence**

For every hit, replace the concept "permanent auto-discovery key from Settings" with "one-time enrollment code generated in Settings → Agents (default 10-min TTL, single use)". Replace env var `SERVERBEE_AUTO_DISCOVERY_KEY` / config `auto_discovery_key` with `SERVERBEE_ENROLLMENT_CODE` / `enrollment_code`. Keep CN and EN wording in sync (same structure, translated). In `agent.mdx` both languages, update the bootstrap example:

```toml
server_url = "https://your-server:9527"
enrollment_code = "<paste the one-time code from Settings → Agents>"
```

Note explicitly: the code is consumed on first successful registration; after that the agent uses its persisted run token and the code is no longer needed.

- [ ] **Step 3: Update install.sh**

Run: `grep -n "discovery\|--discovery-key\|server-url" deploy/install.sh`
Rename the `--discovery-key` flag to `--enrollment-code` (and the variable it sets / the `enrollment_code` it writes into `agent.toml`). Update the usage/help text and the required-arg error message accordingly.

- [ ] **Step 4: Verify no stale references remain**

Run: `grep -rn "auto_discovery_key\|auto-discovery\|AUTO_DISCOVERY" . --include='*.md' --include='*.mdx' --include='*.sh' --include='*.rs' --include='*.toml' | grep -v target/`
Expected: no results (or only this plan file / CHANGELOG).

- [ ] **Step 5: Commit**

```bash
git add ENV.md apps/docs/content deploy/install.sh
git commit -m "docs: replace auto-discovery key with one-time enrollment code flow"
```

---

### Task 10: Frontend — enrollment management + copyable install command

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/index.tsx`
- Modify: the API client module (find with `grep -rn "auto-discovery-key\|autoDiscoveryKey" apps/web/src`)

- [ ] **Step 1: Find current discovery-key UI + API usage**

Run: `grep -rn "auto-discovery-key\|autoDiscoveryKey\|AutoDiscovery\|discovery" apps/web/src`
Read `apps/web/src/routes/_authed/settings/index.tsx` to see the current card and how it calls the API client. This is the code to replace.

- [ ] **Step 2: Replace API calls**

In the API client module, remove `getAutoDiscoveryKey` / `regenerateAutoDiscoveryKey`. Add:

```ts
export const createEnrollment = (body: { label?: string; ttl_secs?: number }) =>
  apiClient.post<{ id: string; code: string; expires_at: string }>(
    '/agent/enrollments',
    body
  )

export const listEnrollments = () =>
  apiClient.get<
    {
      id: string
      label: string | null
      code_prefix: string
      created_by: string
      expires_at: string
      consumed_at: string | null
      created_at: string
    }[]
  >('/agent/enrollments')

export const deleteEnrollment = (id: string) =>
  apiClient.delete(`/agent/enrollments/${id}`)
```

(Match the existing `apiClient` call signature/shape used by neighbouring calls — adjust generics/paths to the real client.)

- [ ] **Step 3: Replace the settings card**

In `apps/web/src/routes/_authed/settings/index.tsx`, replace the discovery-key card with an "Add an agent" card that:
- has a "Generate enrollment code" button (calls `createEnrollment({})`),
- on success shows the plaintext `code` **once** in a copyable block plus a ready-to-paste install command built from `window.location.origin`:

```tsx
const installCmd = `curl -fsSL ${window.location.origin}/install.sh | \\
  sudo bash -s -- --server-url ${window.location.origin} \\
  --enrollment-code ${created.code}`
```

  (Use the real install-script URL/flags from `deploy/install.sh` as updated in Task 9.)
- shows a TanStack Query list of existing enrollments (`listEnrollments`) with prefix, label, expiry, consumed/active badge, and a delete button (`deleteEnrollment`).
- Use shadcn `<Card>`, `<Button>`, copy-to-clipboard, and a shadcn `<ScrollArea>` if the list scrolls (per project convention: no native `overflow-auto`).

Invalidate the list query after create/delete.

- [ ] **Step 4: Lint, typecheck, and visual check**

Run: `cd apps/web && bun x ultracite check && bun run typecheck`
Expected: 0 errors.

Then start the dev stack and verify in a browser (per project testing guidance — this is a behavioral UI change, not CSS-only): generate a code, confirm it shows once, copy the install command, confirm the list shows the new entry with an "active" badge, delete it, confirm it disappears. If you cannot run the browser, say so explicitly rather than claiming success.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src
git commit -m "feat(web): enrollment code management with copyable install command"
```

---

## Self-Review

**Spec coverage** (against the security findings this plan addresses):
- S-H1 (plaintext/echoed/permanent discovery key): Tasks 1 (delete config row), 2 (argon2 hash, never echoed), 4 (consume on use), 7 (remove entirely). ✓
- S-H2 / M1 (IP-only limit + fingerprint takeover via permanent key): Task 4 — takeover now requires an unconsumed, unexpired, single-use code instead of a permanent global secret; existing per-IP `check_register_rate` is retained as defence-in-depth. ✓ (Per-fingerprint rate limiting is out of scope and noted as a residual; the one-time code is the primary mitigation, matching forward-rs' model.)
- Non-constant-time compare: Task 2 — argon2 verify, constant-time, with comment. ✓
- M3 (no revoke/rotate): Task 5. ✓
- L1 (no audit on rotation/registration): Tasks 3, 4, 5 — `agent_enrollment_created/deleted`, `agent_enrolled`, `agent_token_rotated`. ✓

**Placeholder scan:** No "TBD"/"handle errors appropriately". Spots that depend on real codebase names (admin router mount in Task 3, `AgentManager::disconnect` in Task 5, API client signature in Task 10, install-script flags in Task 9) each have an explicit grep step to discover the real name before writing code — these are investigation steps, not placeholders.

**Type consistency:** `EnrollmentService::{mint,verify_and_consume,list,delete,prune}` used consistently across Tasks 2–6. `agent_enrollment::Column::{CodePrefix,ConsumedAt,ExpiresAt,Id}` match the entity in Task 1. Agent field `enrollment_code` consistent across Tasks 8–10. Response field `code` (plaintext, once) consistent in Tasks 3 and 10.

**Residual risks accepted (documented, not fixed here):** enrollment code still travels in the install command (operator-handled, short-lived now); run token still has no TTL (but is now revocable); no mTLS. These mirror forward-rs' own residual weaknesses and are out of scope for this port.
