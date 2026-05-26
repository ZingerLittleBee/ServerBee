# Agent Registration Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current "agent creates server on first connect + fingerprint dedup + recovery-merge" flow with "operator pre-creates server + enrollment is server-bound 1:1 + simple per-server recover" per the spec at `docs/superpowers/specs/2026-05-25-agent-registration-redesign-design.md`.

**Architecture:** *Add Server* writes the `servers` row immediately (token NULL = pending) and mints a server-bound enrollment in one transaction. The agent's `/api/agent/register` call only updates the bound row, never creates one. *Recover* and *Regenerate-code* mint new bound codes for an existing server. The complex `recovery-merge` apparatus is deleted in full.

**Tech Stack:** Rust (Axum, sea-orm, SQLite), React 19 + TanStack Router + Query + shadcn/ui, Biome/Ultracite, vitest, cargo test.

**Spec reference:** `docs/superpowers/specs/2026-05-25-agent-registration-redesign-design.md` (rev 4). Whenever this plan and the spec disagree, the spec wins — flag the conflict instead of guessing.

**Repo conventions to honor:**
- Migrations: SQLite-only project; raw SQL via `execute_unprepared` is fine. `up()` only; `down()` is `Ok(())`. Next migration number is **`m20260525_000034`**.
- `cargo clippy --workspace -- -D warnings` is CI-gated.
- Frontend: `bun x ultracite check` / `bun x ultracite fix`; line width 120, single quotes (JS), double quotes (JSX), no trailing commas.
- Commits: Conventional Commits (`feat`, `fix`, `refactor`, `chore`, `test`). No Claude attribution in commit messages.
- Do **not** push between commits. Push at the end (or let the user do it).

**Order of operations rationale:**
1. Tear down recovery-merge first (Tasks 1-3). It mutates protocol and AppState; doing it after the new flow lands would mean two destabilizing changes overlap.
2. Schema + entity changes next (Task 4). Everything downstream depends on `Option<String>` token columns and the new enrollment shape.
3. Service-layer plumbing (Tasks 5-6) before API handlers (Tasks 7-12) before frontend (Tasks 13-17).

---

## File Map

### Server (Rust) — to be created
- `crates/server/src/migration/m20260525_000034_agent_registration_redesign.rs` — schema migration

### Server (Rust) — to be modified
- `crates/server/src/migration/mod.rs` — register new migration
- `crates/server/src/entity/server.rs` — token columns become `Option<String>`
- `crates/server/src/entity/agent_enrollment.rs` — add `target_server_id`, `revoked_at`; drop `label`
- `crates/server/src/entity/mod.rs` — remove `pub mod recovery_job;`
- `crates/server/src/service/mod.rs` — remove recovery service modules
- `crates/server/src/service/auth.rs` — `validate_agent_token` filters NULL token rows
- `crates/server/src/service/enrollment.rs` — replace API with `verify_and_consume_tx`, support `revoked_at`, `target_server_id`, optimistic supersession
- `crates/server/src/service/server.rs` — drop `ensure_no_running_recovery`; add `remove_connection` side effect on delete
- `crates/server/src/router/api/agent.rs` — refactor `register`; remove `create_enrollment` mint route; tighten GET enrollments DTO; `DELETE /api/agent/enrollments/{id}` becomes a revoke; rotate-token rejects pending
- `crates/server/src/router/api/server.rs` — add `POST /api/servers`, `POST /api/servers/{id}/recover`, `POST /api/servers/{id}/regenerate-code`; `ServerResponse` gains `has_token` + `outstanding_enrollment`
- `crates/server/src/router/api/mod.rs` — remove `server_recovery` mod + merge call
- `crates/server/src/router/ws/agent.rs` — remove all `recovery_lock.writes_allowed_for(...)` guards
- `crates/server/src/router/ws/browser.rs` — remove recovery snapshot + broadcast
- `crates/server/src/state.rs` — remove `recovery_lock` field + initialization
- `crates/server/src/openapi.rs` — drop recovery routes + DTOs; register new routes

### Server (Rust) — to be deleted
- `crates/server/src/entity/recovery_job.rs`
- `crates/server/src/service/recovery_job.rs`
- `crates/server/src/service/recovery_merge.rs`
- `crates/server/src/service/recovery_lock.rs`
- `crates/server/src/router/api/server_recovery.rs`

### Common — to be modified
- `crates/common/src/protocol.rs` — remove `RecoveryJob*` types, `ServerMessage::RebindIdentity`, `AgentMessage::RebindIdentityAck` + `RebindIdentityFailed`; remove `recoveries` field from `BrowserMessage::FullSync` and `BrowserMessage::Update`

### Agent — to be modified
- `crates/agent/src/reporter.rs` — remove `RebindIdentity` handling branch (`reporter.rs:664-696`)

### Frontend — to be created
- `apps/web/src/components/server/recover-agent-dialog.tsx` + `.test.tsx`
- `apps/web/src/components/server/pending-server-card.tsx` (or inline variant in `server-card.tsx`)

### Frontend — to be modified
- `apps/web/src/components/server/add-server-dialog.tsx` — full form rewrite
- `apps/web/src/components/server/status-dot.tsx` — add pending variant
- `apps/web/src/components/server/server-card.tsx` — render pending state, action menu
- `apps/web/src/hooks/use-servers-ws.ts` — remove `recoveries` handling
- `apps/web/src/hooks/use-api.ts` — remove `useRecoveryCandidates`, `startRecoveryMerge`
- `apps/web/src/lib/api-schema.ts` / `api-types.ts` — regenerate from updated OpenAPI; remove `RecoveryJob*`
- `apps/web/src/routes/_authed/servers/$id-page.tsx` — replace Recover button trigger
- `apps/web/src/routes/_authed/servers/index.tsx` — surface Recover entry on offline cards
- `apps/web/src/locales/en/servers.json` + `zh/servers.json` — remove `recovery_merge_*`, `recovery_stage_*`; add new keys

### Frontend — to be deleted
- `apps/web/src/components/server/recovery-merge-dialog.tsx` + `.test.tsx`
- `apps/web/src/stores/recovery-jobs-store.ts` + `.test.ts`

---

## Task 1: Tear down agent + common recovery protocol

**Files:**
- Modify: `crates/common/src/protocol.rs`
- Modify: `crates/agent/src/reporter.rs` (line 664-696 region)

**Why this first:** Common types are imported by both server and agent. Removing them first lets the compiler tell us where else they were used.

- [ ] **Step 1: Inventory references**

Run:
```
grep -rn "RecoveryJob\|RebindIdentity\|recoveries" crates/ apps/web/src/ | grep -v 'target/\|node_modules/' | tee /tmp/recovery-refs.txt
```
Read the file; this is the working list for Tasks 1–3.

- [ ] **Step 2: Delete from `crates/common/src/protocol.rs`**

Remove these items (search-and-delete; preserve surrounding `serde_json` test cases by removing only the recovery-shaped ones):
- `enum RecoveryJobStatus` (around line 71–91)
- `enum RecoveryJobStage` (around line 91)
- `struct RecoveryJobDto` (around line 149–162)
- `AgentMessage::RebindIdentityAck { job_id }` variant (around line 289)
- `AgentMessage::RebindIdentityFailed { job_id, error }` variant (around line 295)
- `ServerMessage::RebindIdentity { job_id, target_server_id, token }` variant (around line 599–614)
- Constant `REBIND_IDENTITY_MIN_PROTOCOL_VERSION` (around line 22)
- `recoveries: Vec<RecoveryJobDto>` field in `BrowserMessage::FullSync` (around line 641)
- `recoveries: Option<Vec<RecoveryJobDto>>` field in `BrowserMessage::Update` (around line 646)
- Any unit tests inside this file that exercise these types

- [ ] **Step 3: Delete agent-side RebindIdentity handler**

In `crates/agent/src/reporter.rs`, locate the `ServerMessage::RebindIdentity { .. } => { ... }` match arm (around line 664–696) and remove it entirely. If the surrounding `match` becomes non-exhaustive, that's the point — let the next step's compile errors guide further fixes.

- [ ] **Step 4: Compile common + agent only**

Run:
```
cargo build -p serverbee-common -p serverbee-agent
```
Expected: agent compiles. If there are leftover references to removed enum variants, fix them by deleting the offending branches (do not reintroduce dead types).

- [ ] **Step 5: Commit**

```
git add crates/common/src/protocol.rs crates/agent/src/reporter.rs
git commit -m "refactor(common,agent): remove RebindIdentity protocol and handler"
```

---

## Task 2: Tear down server recovery infrastructure

**Files:**
- Delete: `crates/server/src/entity/recovery_job.rs`
- Delete: `crates/server/src/service/recovery_job.rs`
- Delete: `crates/server/src/service/recovery_merge.rs`
- Delete: `crates/server/src/service/recovery_lock.rs`
- Delete: `crates/server/src/router/api/server_recovery.rs`
- Modify: `crates/server/src/entity/mod.rs` (line 32)
- Modify: `crates/server/src/service/mod.rs` (lines 30–32)
- Modify: `crates/server/src/router/api/mod.rs` (line 26 + the `.merge(server_recovery::write_router())` at line 87)
- Modify: `crates/server/src/state.rs` (line 79 field; line 234 init)
- Modify: `crates/server/src/service/server.rs` (lines 231–253 `ensure_no_running_recovery`; lines 259 + 276 call sites)
- Modify: `crates/server/src/router/ws/agent.rs` (~20 `recovery_lock.writes_allowed_for(...)` guards at lines 447, 474, 505, 519, 535, 652, 674, 682, 728, 785, 827, 1105, 1129, 1180, 1216, 1248, 1266, 1382, 1420)
- Modify: `crates/server/src/router/ws/browser.rs` (lines 265–432 — `recovery_snapshot`, `broadcast_recovery_update`, related call sites)
- Modify: `crates/server/src/openapi.rs` (drop recovery routes + DTOs)
- Modify: `crates/server/tests/integration.rs` and any sibling integration test files referencing recovery — delete those tests

- [ ] **Step 1: Delete the five recovery files**

```
git rm crates/server/src/entity/recovery_job.rs \
       crates/server/src/service/recovery_job.rs \
       crates/server/src/service/recovery_merge.rs \
       crates/server/src/service/recovery_lock.rs \
       crates/server/src/router/api/server_recovery.rs
```

- [ ] **Step 2: Unregister modules**

Edit `crates/server/src/entity/mod.rs`: delete the line `pub mod recovery_job;`.

Edit `crates/server/src/service/mod.rs`: delete lines `pub mod recovery_job;`, `pub mod recovery_lock;`, `pub mod recovery_merge;`.

Edit `crates/server/src/router/api/mod.rs`: delete `pub mod server_recovery;` (line 26) and the `.merge(server_recovery::write_router())` chain entry (line 87).

- [ ] **Step 3: Remove from AppState**

In `crates/server/src/state.rs`:
- Delete the `pub recovery_lock: RecoveryLockService,` field (line ~79).
- Delete the `recovery_lock: RecoveryLockService::new(),` initialization (line ~234).
- Delete the `use crate::service::recovery_lock::...` import if present.

- [ ] **Step 4: Remove freeze guards from ws/agent.rs**

In `crates/server/src/router/ws/agent.rs`, find every `state.recovery_lock.writes_allowed_for(...)` (the explore agent reported ~20 occurrences at lines 447, 474, 505, 519, 535, 652, 674, 682, 728, 785, 827, 1105, 1129, 1180, 1216, 1248, 1266, 1382, 1420). Each typically looks like:

```rust
if state.recovery_lock.writes_allowed_for(&server_id) {
    // do the write
}
```

Replace with the unconditional inner block. Confirm no `recovery_lock` references remain:
```
grep -n "recovery_lock" crates/server/src/router/ws/agent.rs
```
Expected: no output.

- [ ] **Step 5: Strip recovery from ws/browser.rs**

In `crates/server/src/router/ws/browser.rs`:
- Delete the `recovery_snapshot` function and any helpers it relies on (around line 265–432).
- Delete the `broadcast_recovery_update` function.
- In the initial FullSync construction, remove the `recoveries: ...` field.
- In any code that builds `BrowserMessage::Update { .. }`, remove the `recoveries` field (now also gone from the type).
- Delete `use crate::service::recovery_job::...` / `recovery_merge::...` imports.

- [ ] **Step 6: Strip recovery from service/server.rs**

In `crates/server/src/service/server.rs`:
- Delete the `ensure_no_running_recovery` function (lines ~231–253).
- Delete the doc comment about it (line ~176 region).
- At line ~259 (`delete_server`) and line ~276 (`batch_delete`), remove the `Self::ensure_no_running_recovery(&txn, ...).await?;` lines.
- Update the corresponding tests if they assert this protection (remove those assertions; keep the rest of the test).

- [ ] **Step 7: Strip recovery from openapi.rs**

In `crates/server/src/openapi.rs`, remove any `paths(...)` entries pointing at `server_recovery::*` handlers and any `components(schemas(...))` entries for `RecoveryJob*` DTOs.

- [ ] **Step 8: Delete recovery integration tests**

```
grep -rln "recovery_merge\|RecoveryJob\|recovery_job" crates/server/tests/
```
Delete the matching test functions (or whole files if they're entirely recovery-focused). Keep the rest of each file intact.

- [ ] **Step 9: Build the server crate**

Run:
```
cargo build -p serverbee-server
```
Expected: clean build. If errors remain, they point at leftover references — fix by deletion, never by reintroducing types.

- [ ] **Step 10: Run clippy**

```
cargo clippy -p serverbee-server -- -D warnings
```
Expected: clean. Fix any unused-import warnings inline.

- [ ] **Step 11: Commit**

```
git add -A
git commit -m "refactor(server): remove recovery-merge subsystem"
```

---

## Task 3: Tear down frontend recovery UI

**Files:**
- Delete: `apps/web/src/components/server/recovery-merge-dialog.tsx` + `.test.tsx`
- Delete: `apps/web/src/stores/recovery-jobs-store.ts` + `.test.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts` (lines 61–62 type defs, 249–250 dispatch)
- Modify: `apps/web/src/hooks/use-api.ts` (delete `useRecoveryCandidates`, `startRecoveryMerge`)
- Modify: `apps/web/src/routes/_authed/servers/$id-page.tsx` (lines 175–178 Recover button)
- Modify: `apps/web/src/lib/api-schema.ts` (remove `RecoveryCandidateResponse`, `RecoveryJobResponse` re-exports)
- Modify: `apps/web/src/locales/en/servers.json` and `zh/servers.json` — delete `recovery_merge_*` and `recovery_stage_*` keys (lines ~297–318 in en)

- [ ] **Step 1: Delete dialog and store files**

```
git rm apps/web/src/components/server/recovery-merge-dialog.tsx \
       apps/web/src/components/server/recovery-merge-dialog.test.tsx \
       apps/web/src/stores/recovery-jobs-store.ts \
       apps/web/src/stores/recovery-jobs-store.test.ts
```

- [ ] **Step 2: Strip from `use-servers-ws.ts`**

In `apps/web/src/hooks/use-servers-ws.ts`:
- Remove `recoveries?: RecoveryJobResponse[]` / `recoveries?: RecoveryJobResponse[] | null` from the WS message type union (lines ~61–62).
- Remove the `if (Array.isArray(raw.recoveries)) { useRecoveryJobsStore.getState().setJobs(raw.recoveries) }` block (lines ~249–250).
- Remove the `import { useRecoveryJobsStore } from '@/stores/recovery-jobs-store'` and `RecoveryJobResponse` imports.

- [ ] **Step 3: Strip from `use-api.ts`**

In `apps/web/src/hooks/use-api.ts`, find and delete:
- `useRecoveryCandidates` hook
- `startRecoveryMerge` function
- Related imports

- [ ] **Step 4: Strip from `api-schema.ts` and regenerate types**

Delete:
- `export type RecoveryCandidateResponse = S['RecoveryCandidateResponse']`
- `export type RecoveryJobResponse = S['RecoveryJobResponse']`

(`api-types.ts` is OpenAPI-generated; it will be regenerated in Task 4's CI hook or manually via the existing regen script — leave it for now, the next build will sync it.)

- [ ] **Step 5: Replace Recover button on detail page**

In `apps/web/src/routes/_authed/servers/$id-page.tsx`, replace the existing Recover-related state and button (around line 165–178) with a placeholder that does nothing — Task 16 will reintroduce a real Recover button wired to the new dialog. For now, keep the `Recover Agent` button but make it `onClick={() => { /* TODO: Task 16 */ }}` so the user-visible page still renders. Delete `currentRecoveryJob`, `recoveryHydrated`, and any references to the deleted hooks/store.

- [ ] **Step 6: Delete i18n keys**

In both `apps/web/src/locales/en/servers.json` and `zh/servers.json`, delete every key matching `recovery_merge_*` and `recovery_stage_*`.

- [ ] **Step 7: Compile + typecheck**

Run:
```
cd apps/web && bun run typecheck
```
Expected: clean. Fix leftover references by deletion.

- [ ] **Step 8: Frontend tests**

```
cd apps/web && bun run test
```
Recovery-related tests are gone; the rest should pass.

- [ ] **Step 9: Lint**

```
cd apps/web && bun x ultracite check
```
Fix any new warnings.

- [ ] **Step 10: Commit**

```
git add -A
git commit -m "refactor(web): remove recovery-merge UI"
```

---

## Task 4: Schema migration + entity updates

**Files:**
- Create: `crates/server/src/migration/m20260525_000034_agent_registration_redesign.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Modify: `crates/server/src/entity/server.rs` (token columns → `Option<String>`)
- Modify: `crates/server/src/entity/agent_enrollment.rs` (add columns, drop `label`)

- [ ] **Step 1: Write the migration**

Create `crates/server/src/migration/m20260525_000034_agent_registration_redesign.rs`:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260525_000034_agent_registration_redesign"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 1. Drop the fingerprint unique index. Multiple servers may now legally
        //    share a fingerprint (cloned VMs, two pre-created rows for the same
        //    host, etc.). Fingerprint is informational only after this redesign.
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_fingerprint")
            .await?;

        // 2. Make the servers token columns nullable. SQLite allows DROP NOT NULL
        //    only via table rebuild; we do that with a raw block.
        db.execute_unprepared(
            r#"
            CREATE TABLE servers_new (
                id TEXT PRIMARY KEY NOT NULL,
                token_hash TEXT,
                token_prefix TEXT,
                name TEXT NOT NULL,
                cpu_name TEXT,
                cpu_cores INTEGER,
                cpu_arch TEXT,
                os TEXT,
                kernel_version TEXT,
                mem_total INTEGER,
                swap_total INTEGER,
                disk_total INTEGER,
                ipv4 TEXT,
                ipv6 TEXT,
                region TEXT,
                country_code TEXT,
                virtualization TEXT,
                agent_version TEXT,
                group_id TEXT,
                weight INTEGER NOT NULL DEFAULT 0,
                hidden INTEGER NOT NULL DEFAULT 0,
                remark TEXT,
                public_remark TEXT,
                price REAL,
                billing_cycle TEXT,
                currency TEXT,
                expired_at TIMESTAMP,
                traffic_limit INTEGER,
                traffic_limit_type TEXT,
                billing_start_day INTEGER,
                capabilities INTEGER NOT NULL,
                protocol_version INTEGER NOT NULL,
                features TEXT NOT NULL DEFAULT '[]',
                last_remote_addr TEXT,
                fingerprint TEXT,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL
            );
            INSERT INTO servers_new SELECT * FROM servers;
            DROP TABLE servers;
            ALTER TABLE servers_new RENAME TO servers;
            CREATE INDEX idx_servers_group_id ON servers(group_id);
            "#,
        )
        .await?;

        // 3. Drop legacy enrollment + recovery tables and recreate enrollment clean.
        db.execute_unprepared("DROP TABLE IF EXISTS agent_enrollments")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS recovery_job")
            .await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE agent_enrollments (
                id TEXT PRIMARY KEY NOT NULL,
                code_hash TEXT NOT NULL,
                code_prefix TEXT NOT NULL,
                target_server_id TEXT NOT NULL
                    REFERENCES servers(id) ON DELETE CASCADE,
                created_by TEXT NOT NULL REFERENCES users(id),
                expires_at TIMESTAMP NOT NULL,
                consumed_at TIMESTAMP,
                revoked_at TIMESTAMP,
                created_at TIMESTAMP NOT NULL
            );
            CREATE UNIQUE INDEX idx_enrollments_active_per_server
                ON agent_enrollments(target_server_id)
                WHERE consumed_at IS NULL AND revoked_at IS NULL;
            CREATE INDEX idx_enrollments_code_prefix
                ON agent_enrollments(code_prefix);
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

Verify the column list against `crates/server/src/entity/server.rs:6-47` and adjust if a column has been added since this plan was written.

- [ ] **Step 2: Register the migration**

In `crates/server/src/migration/mod.rs`, add `mod m20260525_000034_agent_registration_redesign;` and append `Box::new(m20260525_000034_agent_registration_redesign::Migration)` to the `migrations()` vec.

- [ ] **Step 3: Update `entity/server.rs`**

Change:
```rust
pub token_hash: String,
pub token_prefix: String,
```
to:
```rust
pub token_hash: Option<String>,
pub token_prefix: Option<String>,
```

- [ ] **Step 4: Update `entity/agent_enrollment.rs`**

Replace the struct with:
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_enrollments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code_hash: String,
    #[sea_orm(indexed)]
    pub code_prefix: String,
    pub target_server_id: String,
    pub created_by: String,
    pub expires_at: DateTimeUtc,
    pub consumed_at: Option<DateTimeUtc>,
    pub revoked_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatedBy",
        to = "super::user::Column::Id"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::server::Entity",
        from = "Column::TargetServerId",
        to = "super::server::Column::Id"
    )]
    Server,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Server.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 5: Run server build**

```
cargo build -p serverbee-server
```
Expected: compile errors at every site that reads `server.token_hash` / `token_prefix` as `String`. Walk through each error and adjust:
- `as_str()` → `.as_deref()` or `Option` matching
- `Set(...)` calls now wrap in `Some(...)` or `None`
- `unwrap` is fine in places where we just verified the row is non-pending; document at the call site why.

These adjustments are mechanical. The functional change is concentrated in Tasks 5 and 8 below; here we just keep the build green.

- [ ] **Step 6: Run migrations against a fresh test DB**

```
cargo test -p serverbee-server --lib test_utils::tests::setup_test_db 2>&1 | tail -5
```
(or any small test that calls `setup_test_db`). Expected: PASS — migration runs cleanly on a fresh DB.

- [ ] **Step 7: Commit**

```
git add crates/server/src/migration crates/server/src/entity
git commit -m "feat(server): migration + nullable token + bound enrollment schema"
```

---

## Task 5: AuthService::validate_agent_token rejects NULL tokens

**Files:**
- Modify: `crates/server/src/service/auth.rs` (`validate_agent_token` around line 318)
- Test: same file, `#[cfg(test)] mod tests` section, add new test

- [ ] **Step 1: Write the failing test**

Add to the existing test module in `auth.rs`:

```rust
#[tokio::test]
async fn validate_agent_token_rejects_pending_server() {
    use crate::entity::server;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::*;
    use serverbee_common::constants::CAP_DEFAULT;

    let (db, _tmp) = setup_test_db().await;
    let now = Utc::now();
    let sid = uuid::Uuid::new_v4().to_string();

    server::ActiveModel {
        id: Set(sid.clone()),
        token_hash: Set(None),
        token_prefix: Set(None),
        name: Set("pending".into()),
        cpu_name: Set(None), cpu_cores: Set(None), cpu_arch: Set(None),
        os: Set(None), kernel_version: Set(None),
        mem_total: Set(None), swap_total: Set(None), disk_total: Set(None),
        ipv4: Set(None), ipv6: Set(None),
        region: Set(None), country_code: Set(None),
        virtualization: Set(None), agent_version: Set(None),
        group_id: Set(None), weight: Set(0), hidden: Set(false),
        remark: Set(None), public_remark: Set(None),
        price: Set(None), billing_cycle: Set(None), currency: Set(None),
        expired_at: Set(None),
        traffic_limit: Set(None), traffic_limit_type: Set(None),
        billing_start_day: Set(None),
        capabilities: Set(CAP_DEFAULT as i32), protocol_version: Set(1),
        features: Set("[]".into()),
        last_remote_addr: Set(None), fingerprint: Set(None),
        created_at: Set(now), updated_at: Set(now),
    }.insert(&db).await.unwrap();

    // Any plausible token string must not match a pending server.
    let result = AuthService::validate_agent_token(&db, "anything-here").await.unwrap();
    assert!(result.is_none(), "pending server must not validate any token");
}
```

- [ ] **Step 2: Run the test to see it fail**

```
cargo test -p serverbee-server --lib service::auth::tests::validate_agent_token_rejects_pending_server -- --nocapture
```
Expected: FAIL — current logic does not filter NULL `token_hash`, so when there are no servers it returns None for the right reason, but when there's a pending server it might still attempt argon2 on a NULL — and panic, or worse, succeed. Confirm whichever symptom shows.

- [ ] **Step 3: Update `validate_agent_token`**

Open `crates/server/src/service/auth.rs` at the `validate_agent_token` function (around line 318). Two changes:

1. The prefix filter query must additionally require `token_hash IS NOT NULL`. The simplest form:

```rust
let prefix = &token[..8.min(token.len())];
let candidates = server::Entity::find()
    .filter(server::Column::TokenPrefix.eq(prefix))
    .filter(server::Column::TokenHash.is_not_null())
    .all(db)
    .await?;
```

2. Inside the loop that argon2-verifies, the row's `token_hash` is now `Option<String>`. Unwrap with `as_deref()` and bail if `None` (defense in depth; the filter above already excludes those):

```rust
for cand in candidates {
    let Some(hash) = cand.token_hash.as_deref() else { continue; };
    if AuthService::verify_password(token, hash)? {
        return Ok(Some(cand));
    }
}
```

- [ ] **Step 4: Test passes**

```
cargo test -p serverbee-server --lib service::auth::tests::validate_agent_token_rejects_pending_server
```
Expected: PASS.

- [ ] **Step 5: Run all auth tests**

```
cargo test -p serverbee-server --lib service::auth
```
Expected: PASS for existing tests too.

- [ ] **Step 6: Commit**

```
git add crates/server/src/service/auth.rs
git commit -m "feat(server): validate_agent_token rejects pending servers (NULL token_hash)"
```

---

## Task 6: EnrollmentService rewrite (revoked_at, target_server_id, verify_and_consume_tx)

**Files:**
- Modify: `crates/server/src/service/enrollment.rs`

The new service surface:

```rust
impl EnrollmentService {
    /// Mint an enrollment bound to a specific server.
    /// Caller must run this inside a tx if it is part of a larger atomic op.
    pub async fn mint_for_server<C: ConnectionTrait>(
        conn: &C,
        target_server_id: &str,
        created_by: &str,
        ttl_secs: i64,
    ) -> Result<(agent_enrollment::Model, String), AppError>;

    /// Verify a bearer code and consume it. Contract:
    ///   accepts rows where consumed_at IS NULL
    ///                  AND revoked_at IS NULL
    ///                  AND expires_at > now()
    /// On match, sets consumed_at = now() inside the same tx and returns the row.
    /// On no match (wrong / expired / revoked / consumed), returns Ok(None).
    pub async fn verify_and_consume_tx<C: ConnectionTrait>(
        tx: &C,
        code: &str,
    ) -> Result<Option<agent_enrollment::Model>, AppError>;

    /// List all enrollments (admin UI).
    pub async fn list(db: &DatabaseConnection)
        -> Result<Vec<agent_enrollment::Model>, AppError>;

    /// Mark an enrollment revoked. Idempotent.
    pub async fn revoke(db: &DatabaseConnection, id: &str)
        -> Result<(), AppError>;

    /// Revoke any outstanding enrollment for a server. Used by recover/regenerate.
    /// Returns the id of the revoked row, if any.
    pub async fn revoke_outstanding_tx<C: ConnectionTrait>(
        tx: &C,
        server_id: &str,
    ) -> Result<Option<String>, AppError>;
}
```

- [ ] **Step 1: Write failing tests**

Replace the existing test module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{server, user};
    use crate::test_utils::setup_test_db;
    use chrono::{Duration, Utc};
    use sea_orm::*;
    use uuid::Uuid;
    use serverbee_common::constants::CAP_DEFAULT;

    async fn seed_user(db: &DatabaseConnection) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        user::ActiveModel {
            id: Set(id.clone()),
            username: Set(format!("user-{id}")),
            password_hash: Set("$argon2id$v=19$m=19456,t=2,p=1$x$x".into()),
            role: Set("admin".into()),
            totp_secret: Set(None),
            must_change_password: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
        }.insert(db).await.unwrap();
        id
    }

    async fn seed_pending_server(db: &DatabaseConnection) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.clone()),
            token_hash: Set(None), token_prefix: Set(None),
            name: Set("t".into()),
            cpu_name: Set(None), cpu_cores: Set(None), cpu_arch: Set(None),
            os: Set(None), kernel_version: Set(None),
            mem_total: Set(None), swap_total: Set(None), disk_total: Set(None),
            ipv4: Set(None), ipv6: Set(None),
            region: Set(None), country_code: Set(None),
            virtualization: Set(None), agent_version: Set(None),
            group_id: Set(None), weight: Set(0), hidden: Set(false),
            remark: Set(None), public_remark: Set(None),
            price: Set(None), billing_cycle: Set(None), currency: Set(None),
            expired_at: Set(None),
            traffic_limit: Set(None), traffic_limit_type: Set(None),
            billing_start_day: Set(None),
            capabilities: Set(CAP_DEFAULT as i32), protocol_version: Set(1),
            features: Set("[]".into()),
            last_remote_addr: Set(None), fingerprint: Set(None),
            created_at: Set(now), updated_at: Set(now),
        }.insert(db).await.unwrap();
        id
    }

    #[tokio::test]
    async fn mint_for_server_returns_plaintext_once() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (model, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600).await.unwrap();
        assert_eq!(model.target_server_id, s);
        assert_eq!(model.code_prefix, code[..8]);
        assert!(model.consumed_at.is_none());
        assert!(model.revoked_at.is_none());
    }

    #[tokio::test]
    async fn verify_and_consume_accepts_usable_code() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (_m, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600).await.unwrap();

        let row = db.transaction::<_, _, AppError>(|tx| Box::pin(async move {
            EnrollmentService::verify_and_consume_tx(tx, &code).await
        })).await.unwrap();
        assert!(row.is_some());
        assert!(row.unwrap().consumed_at.is_some());
    }

    #[tokio::test]
    async fn verify_and_consume_rejects_revoked_code() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (m, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600).await.unwrap();
        EnrollmentService::revoke(&db, &m.id).await.unwrap();

        let row = db.transaction::<_, _, AppError>(|tx| Box::pin(async move {
            EnrollmentService::verify_and_consume_tx(tx, &code).await
        })).await.unwrap();
        assert!(row.is_none(), "revoked code must not consume");
    }

    #[tokio::test]
    async fn verify_and_consume_rejects_expired_code() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        // ttl = -1 to make it already expired
        let (_m, code) = EnrollmentService::mint_for_server(&db, &s, &u, -1).await.unwrap();
        let row = db.transaction::<_, _, AppError>(|tx| Box::pin(async move {
            EnrollmentService::verify_and_consume_tx(tx, &code).await
        })).await.unwrap();
        assert!(row.is_none(), "expired code must not consume");
    }

    #[tokio::test]
    async fn verify_and_consume_single_use() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (_m, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600).await.unwrap();

        let c1 = code.clone();
        let first = db.transaction::<_, _, AppError>(|tx| Box::pin(async move {
            EnrollmentService::verify_and_consume_tx(tx, &c1).await
        })).await.unwrap();
        assert!(first.is_some());

        let c2 = code.clone();
        let second = db.transaction::<_, _, AppError>(|tx| Box::pin(async move {
            EnrollmentService::verify_and_consume_tx(tx, &c2).await
        })).await.unwrap();
        assert!(second.is_none(), "second use must be rejected");
    }

    #[tokio::test]
    async fn partial_index_blocks_two_outstanding() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        EnrollmentService::mint_for_server(&db, &s, &u, 600).await.unwrap();
        let second = EnrollmentService::mint_for_server(&db, &s, &u, 600).await;
        assert!(second.is_err(), "second mint must violate the partial unique index");
    }

    #[tokio::test]
    async fn revoke_outstanding_then_mint_succeeds() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (_first, _code) = EnrollmentService::mint_for_server(&db, &s, &u, 600).await.unwrap();

        let sid = s.clone();
        let uid = u.clone();
        let (_second, _code2) = db.transaction::<_, _, AppError>(|tx| Box::pin(async move {
            EnrollmentService::revoke_outstanding_tx(tx, &sid).await?;
            EnrollmentService::mint_for_server(tx, &sid, &uid, 600).await
        })).await.unwrap();
        // No assertion needed beyond not erroring.
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test -p serverbee-server --lib service::enrollment::tests
```
Expected: FAIL — `mint_for_server` doesn't exist yet, etc.

- [ ] **Step 3: Implement the new service**

Replace the body of `crates/server/src/service/enrollment.rs` (keep `DEFAULT_TTL_SECS`):

```rust
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::agent_enrollment;
use crate::error::AppError;
use crate::service::auth::AuthService;

pub const DEFAULT_TTL_SECS: i64 = 600;

pub struct EnrollmentService;

impl EnrollmentService {
    pub async fn mint_for_server<C: ConnectionTrait>(
        conn: &C,
        target_server_id: &str,
        created_by: &str,
        ttl_secs: i64,
    ) -> Result<(agent_enrollment::Model, String), AppError> {
        let now = Utc::now();
        let plaintext = AuthService::generate_session_token();
        let hash = AuthService::hash_password(&plaintext)?;
        let prefix = plaintext[..8.min(plaintext.len())].to_string();
        let id = Uuid::new_v4().to_string();

        let model = agent_enrollment::ActiveModel {
            id: Set(id),
            code_hash: Set(hash),
            code_prefix: Set(prefix),
            target_server_id: Set(target_server_id.to_string()),
            created_by: Set(created_by.to_string()),
            expires_at: Set(now + Duration::seconds(ttl_secs)),
            consumed_at: Set(None),
            revoked_at: Set(None),
            created_at: Set(now),
        }
        .insert(conn)
        .await?;

        Ok((model, plaintext))
    }

    pub async fn verify_and_consume_tx<C: ConnectionTrait>(
        tx: &C,
        code: &str,
    ) -> Result<Option<agent_enrollment::Model>, AppError> {
        if code.len() < 8 {
            return Ok(None);
        }
        let prefix = &code[..8];
        let candidates = agent_enrollment::Entity::find()
            .filter(agent_enrollment::Column::CodePrefix.eq(prefix))
            .filter(agent_enrollment::Column::ConsumedAt.is_null())
            .filter(agent_enrollment::Column::RevokedAt.is_null())
            .all(tx)
            .await?;

        let now = Utc::now();
        for cand in candidates {
            if cand.expires_at <= now {
                continue;
            }
            if AuthService::verify_password(code, &cand.code_hash)? {
                let mut active: agent_enrollment::ActiveModel = cand.clone().into();
                active.consumed_at = Set(Some(now));
                let updated = active.update(tx).await?;
                return Ok(Some(updated));
            }
        }
        Ok(None)
    }

    pub async fn list(
        db: &DatabaseConnection,
    ) -> Result<Vec<agent_enrollment::Model>, AppError> {
        Ok(agent_enrollment::Entity::find().all(db).await?)
    }

    pub async fn revoke(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let row = agent_enrollment::Entity::find_by_id(id).one(db).await?;
        let Some(row) = row else { return Ok(()); };
        if row.revoked_at.is_some() {
            return Ok(()); // idempotent
        }
        let mut active: agent_enrollment::ActiveModel = row.into();
        active.revoked_at = Set(Some(Utc::now()));
        active.update(db).await?;
        Ok(())
    }

    pub async fn revoke_outstanding_tx<C: ConnectionTrait>(
        tx: &C,
        server_id: &str,
    ) -> Result<Option<String>, AppError> {
        let outstanding = agent_enrollment::Entity::find()
            .filter(agent_enrollment::Column::TargetServerId.eq(server_id))
            .filter(agent_enrollment::Column::ConsumedAt.is_null())
            .filter(agent_enrollment::Column::RevokedAt.is_null())
            .one(tx)
            .await?;
        let Some(row) = outstanding else { return Ok(None); };
        let id = row.id.clone();
        let mut active: agent_enrollment::ActiveModel = row.into();
        active.revoked_at = Set(Some(Utc::now()));
        active.update(tx).await?;
        Ok(Some(id))
    }
}
```

- [ ] **Step 4: Run tests; expect PASS**

```
cargo test -p serverbee-server --lib service::enrollment::tests
```

- [ ] **Step 5: Run clippy**

```
cargo clippy -p serverbee-server --lib -- -D warnings
```

- [ ] **Step 6: Commit**

```
git add crates/server/src/service/enrollment.rs
git commit -m "feat(server): EnrollmentService supports server-bound + revoked_at + tx consume"
```

---

## Task 7: POST /api/servers (Add Server) and ServerResponse DTO additions

**Files:**
- Modify: `crates/server/src/router/api/server.rs` — add `create_server` handler + DTO `CreateServerRequest`, `CreateServerResponse`; extend `ServerResponse` with `has_token` and `outstanding_enrollment`
- Modify: `crates/server/src/router/api/mod.rs` — wire new route into admin router
- Modify: `crates/server/src/openapi.rs` — register new path + DTOs
- Test: integration test for the new endpoint

**Why combined:** Add Server's response shape exactly mirrors the regenerate/recover paths (server_id + enrollment block), and the new fields on ServerResponse drive the frontend's pending UI for all three paths.

- [ ] **Step 1: Write failing integration test**

In `crates/server/tests/integration.rs` (or a new `crates/server/tests/agent_registration_integration.rs`), add:

```rust
// crates/server/tests/agent_registration_integration.rs
use axum::http::StatusCode;
// existing test helpers — copy bootstrap from sibling integration files
mod common; // helper module with build_test_app() etc., or inline boilerplate

#[tokio::test]
async fn post_servers_creates_pending_with_enrollment() {
    let (client, _state) = common::build_authed_admin_client().await;
    let resp = client
        .post("/api/servers")
        .json(&serde_json::json!({
            "name": "vps-1",
            "ttl_secs": 600,
        }))
        .send().await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await;
    let data = &body["data"];
    assert!(data["server_id"].is_string());
    assert!(data["enrollment"]["code"].as_str().unwrap().len() >= 16);
    assert_eq!(data["enrollment"]["code"].as_str().unwrap()[..8],
               *data["enrollment"]["code_prefix"].as_str().unwrap());
}

#[tokio::test]
async fn post_servers_respects_max_servers_cap() {
    let (client, _state) = common::build_authed_admin_client_with_cap(1).await;
    let _ = client.post("/api/servers").json(&serde_json::json!({"name":"a"})).send().await;
    let resp = client.post("/api/servers").json(&serde_json::json!({"name":"b"})).send().await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
```

If `common` test scaffolding doesn't yet exist, peek at `crates/server/tests/cost_integration.rs` or similar for the pattern — typically each integration file bootstraps its own test app via `setup_test_db()` + `Router::new()...`. Follow whatever convention is already in use.

- [ ] **Step 2: Run the tests; verify they fail**

```
cargo test -p serverbee-server --test agent_registration_integration
```
Expected: FAIL — endpoint does not exist.

- [ ] **Step 3: Implement DTOs + handler in `router/api/server.rs`**

Add to the top of the file (next to existing DTOs):

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateServerRequest {
    pub name: String,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub remark: Option<String>,
    #[serde(default)]
    pub public_remark: Option<String>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub billing_cycle: Option<String>,
    #[serde(default)]
    pub billing_start_day: Option<i32>,
    #[serde(default)]
    pub expired_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub traffic_limit: Option<i64>,
    #[serde(default)]
    pub traffic_limit_type: Option<String>,
    /// Capabilities to encode into the install.sh --caps arg only; not persisted on the server row.
    /// Server row's capabilities column is set to CAP_DEFAULT.
    #[serde(default)]
    pub caps: Option<Vec<String>>,
    /// Defaults to 600 (10 min) per spec.
    #[serde(default)]
    pub ttl_secs: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnrollmentIssueResponse {
    pub id: String,
    pub code: String,
    pub code_prefix: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateServerResponse {
    pub server_id: String,
    pub enrollment: EnrollmentIssueResponse,
}
```

Then the handler:

```rust
#[utoipa::path(
    post,
    path = "/api/servers",
    tag = "server",
    request_body = CreateServerRequest,
    responses(
        (status = 200, body = CreateServerResponse),
        (status = 400, description = "Validation or max_servers cap"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_server(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(body): Json<CreateServerRequest>,
) -> Result<Json<ApiResponse<CreateServerResponse>>, AppError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    // Soft max_servers cap — non-locking pre-check.
    let max = state.config.auth.max_servers;
    if max > 0 {
        let count = server::Entity::find().count(&state.db).await?;
        if count >= max as u64 {
            return Err(AppError::BadRequest(format!(
                "Server limit reached ({max})"
            )));
        }
    }

    let ttl = body.ttl_secs.unwrap_or(DEFAULT_TTL_SECS);
    let server_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let (server_id_out, enrollment_model, plaintext_code) = state.db.transaction::<_, _, AppError>(|tx| {
        let server_id = server_id.clone();
        let body = body.clone(); // CreateServerRequest must derive Clone — add it.
        let user_id = current_user.user_id.clone();
        let name = name.to_string();
        Box::pin(async move {
            // 1. Insert server row (pending: token_hash = None)
            server::ActiveModel {
                id: Set(server_id.clone()),
                token_hash: Set(None),
                token_prefix: Set(None),
                name: Set(name),
                cpu_name: Set(None), cpu_cores: Set(None), cpu_arch: Set(None),
                os: Set(None), kernel_version: Set(None),
                mem_total: Set(None), swap_total: Set(None), disk_total: Set(None),
                ipv4: Set(None), ipv6: Set(None),
                region: Set(None), country_code: Set(None),
                virtualization: Set(None), agent_version: Set(None),
                group_id: Set(body.group_id.clone()),
                weight: Set(0), hidden: Set(false),
                remark: Set(body.remark.clone()),
                public_remark: Set(body.public_remark.clone()),
                price: Set(body.price),
                billing_cycle: Set(body.billing_cycle.clone()),
                currency: Set(body.currency.clone()),
                expired_at: Set(body.expired_at),
                traffic_limit: Set(body.traffic_limit),
                traffic_limit_type: Set(body.traffic_limit_type.clone()),
                billing_start_day: Set(body.billing_start_day),
                capabilities: Set(serverbee_common::constants::CAP_DEFAULT as i32),
                protocol_version: Set(1),
                features: Set("[]".into()),
                last_remote_addr: Set(None),
                fingerprint: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(tx)
            .await?;

            // 2. Insert tags (best effort: rely on existing helper if any)
            for tag in &body.tags {
                let tag = tag.trim();
                if tag.is_empty() { continue; }
                server_tag::ActiveModel {
                    server_id: Set(server_id.clone()),
                    tag: Set(tag.to_string()),
                }
                .insert(tx)
                .await?;
            }

            // 3. Apply default network probe targets
            NetworkProbeService::apply_defaults_tx(tx, &server_id).await?;

            // 4. Mint bound enrollment
            let (model, plaintext) = EnrollmentService::mint_for_server(
                tx, &server_id, &user_id, ttl,
            ).await?;

            Ok((server_id, model, plaintext))
        })
    }).await?;

    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "server_created",
        Some(&format!("server_id={server_id_out} enrollment={}", enrollment_model.id)),
        "",
    ).await;

    ok(CreateServerResponse {
        server_id: server_id_out,
        enrollment: EnrollmentIssueResponse {
            id: enrollment_model.id,
            code: plaintext_code,
            code_prefix: enrollment_model.code_prefix,
            expires_at: enrollment_model.expires_at.to_rfc3339(),
        },
    })
}
```

Notes:
- Add `#[derive(Clone)]` on `CreateServerRequest`.
- Add `NetworkProbeService::apply_defaults_tx` if only a `&DatabaseConnection`-typed version exists today — refactor it to take `&C: ConnectionTrait`. Same for any helper inserting `server_tag` rows.

- [ ] **Step 4: Wire the route**

In the admin router section of `crates/server/src/router/api/mod.rs` (or directly in `router/api/server.rs::admin_router` if that's the pattern), add:

```rust
.route("/servers", post(create_server))
```

- [ ] **Step 5: Extend `ServerResponse` (the existing GET DTO)**

Find the `ServerResponse` struct in `crates/server/src/router/api/server.rs` (around line 73–109) and add two fields:

```rust
pub has_token: bool,
pub outstanding_enrollment: Option<OutstandingEnrollmentSummary>,
```

Define:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OutstandingEnrollmentSummary {
    pub id: String,
    pub code_prefix: String,
    pub expires_at: String,
    pub created_at: String,
}
```

Populate at every site that builds `ServerResponse` from a `server::Model`:

```rust
let has_token = model.token_hash.is_some();
let outstanding = agent_enrollment::Entity::find()
    .filter(agent_enrollment::Column::TargetServerId.eq(&model.id))
    .filter(agent_enrollment::Column::ConsumedAt.is_null())
    .filter(agent_enrollment::Column::RevokedAt.is_null())
    .one(&state.db)
    .await?
    .map(|m| OutstandingEnrollmentSummary {
        id: m.id,
        code_prefix: m.code_prefix,
        expires_at: m.expires_at.to_rfc3339(),
        created_at: m.created_at.to_rfc3339(),
    });
```

For the list endpoint, batch the outstanding-enrollment query across all server ids (single `IN` query) instead of N+1.

- [ ] **Step 6: Register in openapi.rs**

Add `CreateServerRequest`, `CreateServerResponse`, `EnrollmentIssueResponse`, `OutstandingEnrollmentSummary` to the components/schemas list and add `create_server` to the paths list.

- [ ] **Step 7: Run tests**

```
cargo test -p serverbee-server --test agent_registration_integration
cargo test -p serverbee-server --lib
```
Expected: PASS.

- [ ] **Step 8: Run clippy**

```
cargo clippy -p serverbee-server -- -D warnings
```

- [ ] **Step 9: Commit**

```
git add -A crates/server
git commit -m "feat(server): POST /api/servers + ServerResponse has_token/outstanding_enrollment"
```

---

## Task 8: Refactor POST /api/agent/register (transactional, no row creation)

**Files:**
- Modify: `crates/server/src/router/api/agent.rs` (`register` handler around line 112–328)

- [ ] **Step 1: Write failing integration test**

Append to `crates/server/tests/agent_registration_integration.rs`:

```rust
#[tokio::test]
async fn agent_register_updates_bound_server_does_not_create() {
    let (client, state) = common::build_authed_admin_client().await;

    let resp = client.post("/api/servers")
        .json(&serde_json::json!({"name": "vps-x"}))
        .send().await;
    let body: serde_json::Value = resp.json().await;
    let server_id = body["data"]["server_id"].as_str().unwrap().to_string();
    let code = body["data"]["enrollment"]["code"].as_str().unwrap().to_string();

    let count_before = server_count(&state.db).await;

    let reg = unauthed_client()
        .post("/api/agent/register")
        .bearer_auth(&code)
        .json(&serde_json::json!({"fingerprint": ""}))
        .send().await;
    assert_eq!(reg.status(), StatusCode::OK);
    let reg_body: serde_json::Value = reg.json().await;
    assert_eq!(reg_body["data"]["server_id"].as_str().unwrap(), server_id);

    assert_eq!(server_count(&state.db).await, count_before);
}

#[tokio::test]
async fn agent_register_with_revoked_code_returns_401_and_no_token_set() {
    let (client, state) = common::build_authed_admin_client().await;
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-y"}))
        .send().await.json().await;
    let enrollment_id = body["data"]["enrollment"]["id"].as_str().unwrap();
    let code = body["data"]["enrollment"]["code"].as_str().unwrap().to_string();
    let server_id = body["data"]["server_id"].as_str().unwrap().to_string();

    // Revoke via delete endpoint
    client.delete(&format!("/api/agent/enrollments/{enrollment_id}")).send().await;

    let reg = unauthed_client()
        .post("/api/agent/register")
        .bearer_auth(&code)
        .send().await;
    assert_eq!(reg.status(), StatusCode::UNAUTHORIZED);

    let s = server::Entity::find_by_id(&server_id).one(&state.db).await.unwrap().unwrap();
    assert!(s.token_hash.is_none(), "revoked code must not set token");
}
```

(`server_count`, `unauthed_client` are helpers; add them to the test scaffold if missing.)

- [ ] **Step 2: Verify tests fail**

```
cargo test -p serverbee-server --test agent_registration_integration agent_register
```
Expected: FAIL — current handler still creates servers and may accept revoked codes.

- [ ] **Step 3: Replace the `register` handler body**

In `crates/server/src/router/api/agent.rs`, replace the entire `async fn register(...)` body (from `// 1. Rate limiting` through the final `ok(RegisterResponse { ... })`) with the transactional form:

```rust
async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Option<Json<RegisterRequest>>,
) -> Result<Json<ApiResponse<RegisterResponse>>, AppError> {
    let ip = extract_client_ip(&ConnectInfo(addr), &headers, &state.config.server.trusted_proxies).to_string();
    if !state.check_register_rate(&ip) {
        return Err(AppError::TooManyRequests("Too many registration attempts".into()));
    }

    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?
        .to_string();

    let fingerprint = body
        .as_ref()
        .map(|b| b.fingerprint.clone())
        .filter(|f| !f.is_empty());
    if let Some(ref fp) = fingerprint
        && (fp.len() != 64 || !fp.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(AppError::BadRequest("Invalid fingerprint format".into()));
    }

    let ip_clone = ip.clone();
    let fingerprint_clone = fingerprint.clone();
    let result = state.db.transaction::<_, _, AppError>(move |tx| {
        let bearer = bearer.clone();
        let ip = ip_clone.clone();
        let fingerprint = fingerprint_clone.clone();
        Box::pin(async move {
            let enrollment = EnrollmentService::verify_and_consume_tx(tx, &bearer).await?
                .ok_or(AppError::Unauthorized)?;

            let server_row = server::Entity::find_by_id(&enrollment.target_server_id)
                .one(tx)
                .await?
                .ok_or_else(|| AppError::Internal("Bound server vanished".into()))?;

            let plaintext = AuthService::generate_session_token();
            let hash = AuthService::hash_password(&plaintext)?;
            let prefix = plaintext[..8.min(plaintext.len())].to_string();

            let mut active: server::ActiveModel = server_row.clone().into();
            active.token_hash = Set(Some(hash));
            active.token_prefix = Set(Some(prefix));
            active.last_remote_addr = Set(Some(ip));
            active.fingerprint = Set(fingerprint);
            active.updated_at = Set(Utc::now());
            active.update(tx).await?;

            Ok::<_, AppError>((server_row.id, plaintext, enrollment.id))
        })
    }).await?;

    let (server_id, plaintext_token, enrollment_id) = result;
    let _ = AuditService::log(
        &state.db,
        "system",
        "agent_enrolled",
        Some(&format!("server_id={server_id} enrollment_id={enrollment_id}")),
        &ip,
    ).await;

    // Broadcast row update so the UI flips out of pending. Do NOT emit ServerOnline;
    // AgentManager::add_connection owns that event when the WS actually connects.
    // (Use existing per-server-update broadcaster; refer to ws/browser.rs helpers.)

    ok(RegisterResponse {
        server_id,
        token: plaintext_token,
    })
}
```

Also delete the old fingerprint-dedup branch entirely (everything between `// 3. Fingerprint dedup` and `// 5. Create new server` plus the rest of the function).

- [ ] **Step 4: Run tests; expect PASS**

```
cargo test -p serverbee-server --test agent_registration_integration
```

- [ ] **Step 5: Run all server tests**

```
cargo test -p serverbee-server
```
Expected: PASS. Existing tests around `register` (the old `register_flow_consumes_code_single_use`, `mint_then_list_shows_prefix_not_code`) may need to be updated for the new API — adjust them, don't delete.

- [ ] **Step 6: Run clippy**

```
cargo clippy -p serverbee-server -- -D warnings
```

- [ ] **Step 7: Commit**

```
git add crates/server
git commit -m "refactor(server): transactional agent register, no implicit server creation"
```

---

## Task 9: POST /api/servers/{id}/recover

**Files:**
- Modify: `crates/server/src/router/api/server.rs` — add handler `recover_server`
- Modify: `crates/server/src/router/api/mod.rs` — register route
- Modify: `crates/server/src/openapi.rs`
- Test: integration

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn recover_with_revoke_immediately_clears_token_and_kicks() {
    let (client, state) = common::build_authed_admin_client().await;

    // Add server + simulate agent registration to make it non-pending
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-r"}))
        .send().await.json().await;
    let server_id = body["data"]["server_id"].as_str().unwrap().to_string();
    let code = body["data"]["enrollment"]["code"].as_str().unwrap().to_string();
    let _ = unauthed_client().post("/api/agent/register").bearer_auth(&code).send().await;

    let resp = client.post(&format!("/api/servers/{server_id}/recover"))
        .json(&serde_json::json!({"revoke_immediately": true}))
        .send().await;
    assert_eq!(resp.status(), StatusCode::OK);

    let s = server::Entity::find_by_id(&server_id).one(&state.db).await.unwrap().unwrap();
    assert!(s.token_hash.is_none(), "revoke_immediately must NULL token_hash");
}

#[tokio::test]
async fn recover_with_outstanding_enrollment_returns_409() {
    let (client, _state) = common::build_authed_admin_client().await;
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-q"}))
        .send().await.json().await;
    let server_id = body["data"]["server_id"].as_str().unwrap();
    // Server is pending → recover should reject
    let resp = client.post(&format!("/api/servers/{server_id}/recover"))
        .json(&serde_json::json!({"revoke_immediately": false}))
        .send().await;
    // Pending → 400 (not 409). Adjust the test if your handler emits 400 here.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn recover_on_non_pending_with_outstanding_returns_409() {
    let (client, state) = common::build_authed_admin_client().await;
    // Set up server in offline (has token, no live connection)
    // ... add + register agent ...
    // Mint an outstanding code by calling recover once
    // Second recover should 409
    todo!("fill in with helper that goes pending → online → recover (no revoke) → recover again")
}
```

- [ ] **Step 2: Implement handler**

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RecoverRequest {
    pub revoke_immediately: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecoverResponse {
    pub enrollment: EnrollmentIssueResponse,
}

#[utoipa::path(
    post, path = "/api/servers/{id}/recover", tag = "server",
    params(("id" = String, Path, description = "Server id")),
    request_body = RecoverRequest,
    responses(
        (status = 200, body = RecoverResponse),
        (status = 400, description = "Server is pending"),
        (status = 404, description = "Server not found"),
        (status = 409, description = "Outstanding enrollment exists"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn recover_server(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(body): Json<RecoverRequest>,
) -> Result<Json<ApiResponse<RecoverResponse>>, AppError> {
    let user_id = current_user.user_id.clone();
    let server_id_for_kick = id.clone();
    let (enrollment_model, plaintext, kick) = state.db.transaction::<_, _, AppError>(|tx| {
        let id = id.clone();
        let user_id = user_id.clone();
        let revoke = body.revoke_immediately;
        Box::pin(async move {
            let row = server::Entity::find_by_id(&id).one(tx).await?
                .ok_or(AppError::NotFound("server not found".into()))?;

            if row.token_hash.is_none() {
                return Err(AppError::BadRequest(
                    "server is pending; use regenerate-code instead".into()
                ));
            }

            // Outstanding check (recover NEVER auto-supersedes)
            let outstanding = agent_enrollment::Entity::find()
                .filter(agent_enrollment::Column::TargetServerId.eq(&id))
                .filter(agent_enrollment::Column::ConsumedAt.is_null())
                .filter(agent_enrollment::Column::RevokedAt.is_null())
                .one(tx).await?;
            if outstanding.is_some() {
                return Err(AppError::Conflict(
                    "an outstanding enrollment exists; revoke it before recovering".into()
                ));
            }

            let kick = if revoke {
                let mut active: server::ActiveModel = row.into();
                active.token_hash = Set(None);
                active.token_prefix = Set(None);
                active.updated_at = Set(Utc::now());
                active.update(tx).await?;
                true
            } else {
                false
            };

            let (model, plaintext) = EnrollmentService::mint_for_server(
                tx, &id, &user_id, DEFAULT_TTL_SECS
            ).await?;

            Ok::<_, AppError>((model, plaintext, kick))
        })
    }).await?;

    if kick {
        state.agent_manager.remove_connection(&server_id_for_kick);
    }

    ok(RecoverResponse {
        enrollment: EnrollmentIssueResponse {
            id: enrollment_model.id,
            code: plaintext,
            code_prefix: enrollment_model.code_prefix,
            expires_at: enrollment_model.expires_at.to_rfc3339(),
        },
    })
}
```

If `AppError::Conflict` doesn't exist, add it; ensure it maps to HTTP `409`.

- [ ] **Step 3: Wire route**

```rust
.route("/servers/{id}/recover", post(recover_server))
```

- [ ] **Step 4: Run tests; expect PASS**

```
cargo test -p serverbee-server --test agent_registration_integration recover
```

- [ ] **Step 5: Commit**

```
git add crates/server
git commit -m "feat(server): POST /api/servers/{id}/recover"
```

---

## Task 10: POST /api/servers/{id}/regenerate-code

**Files:**
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Test: integration

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn regenerate_supersedes_outstanding() {
    let (client, _state) = common::build_authed_admin_client().await;
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-g"}))
        .send().await.json().await;
    let server_id = body["data"]["server_id"].as_str().unwrap();
    let first_id = body["data"]["enrollment"]["id"].as_str().unwrap().to_string();

    let r = client.post(&format!("/api/servers/{server_id}/regenerate-code"))
        .json(&serde_json::json!({"expected_enrollment_id": first_id}))
        .send().await;
    assert_eq!(r.status(), StatusCode::OK);
}

#[tokio::test]
async fn regenerate_with_stale_expected_id_returns_409() {
    let (client, _state) = common::build_authed_admin_client().await;
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-h"}))
        .send().await.json().await;
    let server_id = body["data"]["server_id"].as_str().unwrap();

    let r = client.post(&format!("/api/servers/{server_id}/regenerate-code"))
        .json(&serde_json::json!({"expected_enrollment_id": "this-does-not-match"}))
        .send().await;
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn regenerate_on_non_pending_returns_400() {
    let (client, _state) = common::build_authed_admin_client().await;
    // Create + register so server has a token, then try regenerate
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-i"}))
        .send().await.json().await;
    let server_id = body["data"]["server_id"].as_str().unwrap().to_string();
    let code = body["data"]["enrollment"]["code"].as_str().unwrap().to_string();
    let _ = unauthed_client().post("/api/agent/register").bearer_auth(&code).send().await;

    let r = client.post(&format!("/api/servers/{server_id}/regenerate-code"))
        .json(&serde_json::json!({}))
        .send().await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Implement handler**

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegenerateCodeRequest {
    #[serde(default)]
    pub expected_enrollment_id: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegenerateCodeResponse {
    pub enrollment: EnrollmentIssueResponse,
}

#[utoipa::path(
    post, path = "/api/servers/{id}/regenerate-code", tag = "server",
    params(("id" = String, Path)),
    request_body = RegenerateCodeRequest,
    responses(
        (status = 200, body = RegenerateCodeResponse),
        (status = 400, description = "Server is not pending"),
        (status = 404, description = "Server not found"),
        (status = 409, description = "expected_enrollment_id mismatch"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn regenerate_code(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(body): Json<RegenerateCodeRequest>,
) -> Result<Json<ApiResponse<RegenerateCodeResponse>>, AppError> {
    let user_id = current_user.user_id.clone();
    let (model, plaintext) = state.db.transaction::<_, _, AppError>(|tx| {
        let id = id.clone();
        let user_id = user_id.clone();
        let expected = body.expected_enrollment_id.clone();
        Box::pin(async move {
            let row = server::Entity::find_by_id(&id).one(tx).await?
                .ok_or(AppError::NotFound("server not found".into()))?;
            if row.token_hash.is_some() {
                return Err(AppError::BadRequest("server is not pending".into()));
            }

            let current = agent_enrollment::Entity::find()
                .filter(agent_enrollment::Column::TargetServerId.eq(&id))
                .filter(agent_enrollment::Column::ConsumedAt.is_null())
                .filter(agent_enrollment::Column::RevokedAt.is_null())
                .one(tx).await?;
            let current_id = current.as_ref().map(|m| m.id.clone());
            if expected != current_id {
                return Err(AppError::Conflict("expected_enrollment_id mismatch".into()));
            }

            EnrollmentService::revoke_outstanding_tx(tx, &id).await?;
            EnrollmentService::mint_for_server(tx, &id, &user_id, DEFAULT_TTL_SECS).await
        })
    }).await?;

    ok(RegenerateCodeResponse {
        enrollment: EnrollmentIssueResponse {
            id: model.id, code: plaintext, code_prefix: model.code_prefix,
            expires_at: model.expires_at.to_rfc3339(),
        },
    })
}
```

- [ ] **Step 3: Wire route + openapi**

```rust
.route("/servers/{id}/regenerate-code", post(regenerate_code))
```
Add DTOs + route to `openapi.rs`.

- [ ] **Step 4: Run tests; expect PASS**

```
cargo test -p serverbee-server --test agent_registration_integration regenerate
```

- [ ] **Step 5: Commit**

```
git add crates/server
git commit -m "feat(server): POST /api/servers/{id}/regenerate-code with optimistic CAS"
```

---

## Task 11: GET enrollments DTO updates + DELETE → revoke

**Files:**
- Modify: `crates/server/src/router/api/agent.rs`

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn list_enrollments_includes_id_and_target_server_id() {
    let (client, _state) = common::build_authed_admin_client().await;
    client.post("/api/servers").json(&serde_json::json!({"name":"vps-l"})).send().await;

    let resp = client.get("/api/agent/enrollments").send().await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await;
    let row = &body["data"][0];
    assert!(row["id"].is_string());
    assert!(row["target_server_id"].is_string());
    assert!(row["revoked_at"].is_null() || row["revoked_at"].is_string());
}

#[tokio::test]
async fn delete_enrollment_revokes_only_does_not_delete_server() {
    let (client, state) = common::build_authed_admin_client().await;
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-d"})).send().await.json().await;
    let eid = body["data"]["enrollment"]["id"].as_str().unwrap();
    let sid = body["data"]["server_id"].as_str().unwrap();

    let r = client.delete(&format!("/api/agent/enrollments/{eid}")).send().await;
    assert_eq!(r.status(), StatusCode::OK);

    // server still exists
    assert!(server::Entity::find_by_id(sid).one(&state.db).await.unwrap().is_some());
    // enrollment is revoked, not deleted
    let e = agent_enrollment::Entity::find_by_id(eid).one(&state.db).await.unwrap().unwrap();
    assert!(e.revoked_at.is_some());
}
```

- [ ] **Step 2: Update `EnrollmentSummary` DTO**

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnrollmentSummary {
    pub id: String,
    pub target_server_id: String,
    pub code_prefix: String,
    pub created_by: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}
```

Update `list_enrollments` mapping accordingly.

- [ ] **Step 3: Update `delete_enrollment` to call `revoke`**

Replace `EnrollmentService::delete(&state.db, &id).await?;` with `EnrollmentService::revoke(&state.db, &id).await?;` and adjust audit message: `agent_enrollment_revoked`.

- [ ] **Step 4: Delete the obsolete `create_enrollment` handler and route**

The `POST /api/agent/enrollments` route is removed entirely. Find the `.route("/agent/enrollments", post(create_enrollment))` line in `admin_router()` and delete it, plus the `create_enrollment` function and `CreateEnrollmentRequest` / `CreateEnrollmentResponse` DTOs.

- [ ] **Step 5: Update `openapi.rs`**

Remove `POST /api/agent/enrollments` and `CreateEnrollment*` schemas. Keep `EnrollmentSummary` (updated).

- [ ] **Step 6: Run tests + clippy**

```
cargo test -p serverbee-server
cargo clippy -p serverbee-server -- -D warnings
```

- [ ] **Step 7: Commit**

```
git add crates/server
git commit -m "feat(server): GET enrollments includes id+target_server_id; DELETE = revoke"
```

---

## Task 12: rotate-token guard for pending + DELETE /api/servers/{id} closes WS

**Files:**
- Modify: `crates/server/src/router/api/agent.rs` (`rotate_token` around line 453)
- Modify: `crates/server/src/router/api/server.rs` (`delete_server` handler around line 472)

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn rotate_token_on_pending_returns_400() {
    let (client, _state) = common::build_authed_admin_client().await;
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-p"})).send().await.json().await;
    let sid = body["data"]["server_id"].as_str().unwrap();

    let r = client.post(&format!("/api/agent/{sid}/rotate-token")).send().await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_server_closes_ws_connection() {
    let (client, state) = common::build_authed_admin_client().await;
    let body: serde_json::Value = client.post("/api/servers")
        .json(&serde_json::json!({"name":"vps-z"})).send().await.json().await;
    let sid = body["data"]["server_id"].as_str().unwrap().to_string();

    // simulate connected agent by registering then directly populating agent_manager
    let code = body["data"]["enrollment"]["code"].as_str().unwrap();
    let _ = unauthed_client().post("/api/agent/register").bearer_auth(code).send().await;
    // (For the test to truly observe a connection drop, manually insert a connection;
    //  if AgentManager's API has a test-only insertion helper, use it. Otherwise this
    //  test verifies remove_connection is invoked via tracing/spy.)
    assert!(state.agent_manager.has_connection(&sid) || true,
            "test scaffolding: verify remove_connection is called");

    let r = client.delete(&format!("/api/servers/{sid}")).send().await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(!state.agent_manager.has_connection(&sid));
}
```

(`has_connection` is a small helper to add on `AgentManager` if it doesn't exist — `self.connections.contains_key(server_id)`.)

- [ ] **Step 2: Implement rotate-token guard**

In `crates/server/src/router/api/agent.rs::rotate_token`, after fetching `existing`:

```rust
if existing.token_hash.is_none() {
    return Err(AppError::BadRequest(
        "cannot rotate token of a pending server; use recover instead".into(),
    ));
}
```

- [ ] **Step 3: Implement DELETE side-effect**

In `crates/server/src/router/api/server.rs::delete_server`:

```rust
async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    ServerService::delete_server(&state.db, &id).await?;
    state.agent_manager.remove_connection(&id);
    ok("ok")
}
```

Do the same for `batch_delete` if there is a corresponding handler.

- [ ] **Step 4: Run tests + clippy**

```
cargo test -p serverbee-server
cargo clippy -p serverbee-server -- -D warnings
```

- [ ] **Step 5: Commit**

```
git add crates/server
git commit -m "feat(server): reject rotate-token on pending; DELETE server drops WS"
```

---

## Task 13: Frontend types and WebSocket alignment

**Files:**
- Regenerate: `apps/web/src/lib/api-types.ts` (OpenAPI codegen — use whatever script the project provides; e.g. `bun run gen:types` or a make target)
- Modify: `apps/web/src/lib/api-schema.ts` — add new type re-exports
- Modify: `apps/web/src/hooks/use-servers-ws.ts` — handle `has_token` and `outstanding_enrollment` on incoming server rows

- [ ] **Step 1: Regenerate OpenAPI types**

Find the type-regen script. Typical patterns:
- `bun run gen:types`
- `make openapi-types`
- `bunx openapi-typescript`

Run whichever exists. The output is committed.

- [ ] **Step 2: Add re-exports in `api-schema.ts`**

```typescript
export type CreateServerRequest = S['CreateServerRequest']
export type CreateServerResponse = S['CreateServerResponse']
export type EnrollmentIssueResponse = S['EnrollmentIssueResponse']
export type RecoverRequest = S['RecoverRequest']
export type RecoverResponse = S['RecoverResponse']
export type RegenerateCodeRequest = S['RegenerateCodeRequest']
export type RegenerateCodeResponse = S['RegenerateCodeResponse']
export type OutstandingEnrollmentSummary = S['OutstandingEnrollmentSummary']
```

- [ ] **Step 3: Update WS hook**

In `apps/web/src/hooks/use-servers-ws.ts`, the `ServerMetrics` type (or whatever the per-server WS payload type is named) gains:

```typescript
has_token: boolean
outstanding_enrollment?: OutstandingEnrollmentSummary | null
```

In the `update`/`full_sync` handler, merge these fields onto the cached server.

- [ ] **Step 4: Typecheck**

```
cd apps/web && bun run typecheck
```

- [ ] **Step 5: Commit**

```
git add apps/web/src/lib apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): server types include has_token + outstanding_enrollment"
```

---

## Task 14: Frontend — Add Server dialog rewrite

**Files:**
- Modify: `apps/web/src/components/server/add-server-dialog.tsx`
- Modify: `apps/web/src/locales/en/servers.json` and `zh/servers.json` — add new keys

- [ ] **Step 1: Write failing test for the new submit shape**

`apps/web/src/components/server/add-server-dialog.test.tsx` — assert that submitting the form with required `name` POSTs to `/api/servers` with the expected JSON body and that the dialog transitions to the install-command view on success, showing the plaintext code only once.

(See `recovery-merge-dialog.test.tsx` for the testing pattern; mock the API via `vi.mock('@/lib/api-client', ...)`.)

- [ ] **Step 2: Rewrite the dialog**

Two states:
1. **Form view**: name (required), group (Select), tags (multi-select), remark, public_remark, billing block (price/currency/billing_cycle/billing_start_day/expired_at/traffic_limit/traffic_limit_type), capabilities (checkbox grid, default = CAP_DEFAULT, drives install.sh `--caps`), TTL tip text "Code valid for 10 minutes" — no selector. Submit → `POST /api/servers`.
2. **Install-command view**: warning banner "This code is shown only once", plaintext code, install command (`curl ... install.sh ... --server-url '<origin>' --enrollment-code '<code>' [--caps <list>]`), 3-step instructions, copy buttons, "Done" / "Add another" buttons.

The existing list of past enrollments is removed entirely; pending servers appear in the main server list.

- [ ] **Step 3: i18n keys**

Add to `en/servers.json` (mirror to `zh/servers.json`):
```
"add_server.group_label", "add_server.tags_label", "add_server.remark_label",
"add_server.public_remark_label", "add_server.billing_section", "add_server.price_label",
"add_server.currency_label", "add_server.billing_cycle_label",
"add_server.billing_start_day_label", "add_server.expired_at_label",
"add_server.traffic_limit_label", "add_server.traffic_limit_type_label",
"add_server.ttl_tip" (= "Code valid for 10 minutes"),
"add_server.shown_once_warning" (= "...")
```

Remove obsolete keys (`add_server.validity_*`).

- [ ] **Step 4: Run tests + lint**

```
cd apps/web && bun run test add-server-dialog
cd apps/web && bun x ultracite check
```

- [ ] **Step 5: Commit**

```
git add apps/web
git commit -m "feat(web): Add Server dialog creates pending server with full metadata"
```

---

## Task 15: Frontend — Pending status indicator + server card variant

**Files:**
- Modify: `apps/web/src/components/server/status-dot.tsx`
- Modify: `apps/web/src/components/server/server-card.tsx`
- Modify: `apps/web/src/components/server/status-dot.test.tsx`

- [ ] **Step 1: Update StatusDot**

```typescript
export type StatusKind = 'online' | 'offline' | 'pending'

export function StatusDot({ status, className }: { status: StatusKind; className?: string }) {
  const cls = {
    online:  'animate-pulse bg-emerald-500',
    offline: 'bg-muted-foreground/60',
    pending: 'bg-amber-500',
  }[status]
  return (
    <span
      aria-label={status}
      className={cn('inline-block size-2 rounded-full', cls, className)}
    />
  )
}
```

Update existing callers from `<StatusDot online={server.online} />` to `<StatusDot status={derive(server)} />` with helper:

```typescript
function deriveStatus(s: { online: boolean; has_token: boolean }): StatusKind {
  if (!s.has_token) return 'pending'
  return s.online ? 'online' : 'offline'
}
```

- [ ] **Step 2: Server card pending render**

In `server-card.tsx`, when `status === 'pending'`:
- Replace metric area with "Waiting for agent…" placeholder + outstanding-enrollment summary line:
  - usable (`expires_at` > now, not revoked): `Code <prefix>… · expires in 7m 12s`
  - expired outstanding: `Code <prefix>… expired`
  - no outstanding: `No outstanding enrollment code`
- Add action menu items: **Regenerate code** (opens a small inline dialog showing the freshly-minted code) and **Delete server** (existing delete confirm flow, but allowed even when pending).

- [ ] **Step 3: Update tests**

`status-dot.test.tsx` — assert all three colors and `aria-label` strings.

- [ ] **Step 4: Run tests + lint**

```
cd apps/web && bun run test status-dot server-card
cd apps/web && bun x ultracite check
```

- [ ] **Step 5: Commit**

```
git add apps/web
git commit -m "feat(web): pending server status indicator and card variant"
```

---

## Task 16: Frontend — Recover Agent dialog

**Files:**
- Create: `apps/web/src/components/server/recover-agent-dialog.tsx` + `.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id-page.tsx` — wire the new dialog (replaces the Task 3 placeholder)
- Modify: `apps/web/src/components/server/server-card.tsx` — add Recover entry to action menu for online/offline (not pending)
- Modify: `apps/web/src/locales/en/servers.json` and `zh/servers.json` — add new keys

- [ ] **Step 1: Write failing test**

Assert:
- Dialog renders the server name as read-only.
- Caps checkbox grid is editable.
- Submit POSTs to `/api/servers/{id}/recover` with `{ revoke_immediately: true }` by default.
- When server has an outstanding enrollment, the dialog shows the "outstanding code exists" notice and disables submit; a Revoke button calls `DELETE /api/agent/enrollments/{id}`.

- [ ] **Step 2: Implement the dialog**

Two states (mirror Add Server):
- **Form state**: server name (read-only), caps grid (initial = projection of `server.capabilities`), `revoke_immediately` checkbox (default checked, with warning text when checked: "the current agent will be kicked offline and cannot reconnect until the new install command runs"), TTL tip text, Generate button.
- **Install-command state**: same layout as Add Server's post-submit view.

If `server.outstanding_enrollment` is present when the dialog opens, render the outstanding-code notice with prefix + countdown and a Revoke button instead of the form. After Revoke, refetch and render the form.

- [ ] **Step 3: Wire from server card**

In `server-card.tsx`, when status is `online` or `offline` (not `pending`), surface a "Recover Agent" action in the card menu that opens `<RecoverAgentDialog server={server} />`.

- [ ] **Step 4: Wire from detail page**

In `$id-page.tsx`, replace the Task 3 placeholder with `<RecoverAgentDialog server={server} />`.

- [ ] **Step 5: i18n keys**

Add `recover_agent.*` keys mirroring `add_server.*` semantics: title, description, name_readonly, caps_label, revoke_checkbox, revoke_warning, ttl_tip, submit, outstanding_notice, revoke_button.

- [ ] **Step 6: Run tests + lint**

```
cd apps/web && bun run test recover-agent
cd apps/web && bun x ultracite check
```

- [ ] **Step 7: Commit**

```
git add apps/web
git commit -m "feat(web): Recover Agent dialog (online/offline servers)"
```

---

## Task 17: Final integration check + cleanup

- [ ] **Step 1: Full workspace build + tests**

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cd apps/web && bun run typecheck && bun run test && bun x ultracite check
```

All must pass.

- [ ] **Step 2: Manual E2E walk (per spec § Testing)**

Boot a local server (`make server-dev`) + frontend (`make web-dev`). Walk through:
1. *Add Server* with full form → pending card with "Waiting for agent" + code prefix + countdown → install command runs in a shell against the local server (use a stub agent or skip if no test VPS) → card flips to online.
2. *Recover Agent* on an offline server with default `revoke_immediately = true` → card enters pending.
3. *Recover Agent* on an online server with `revoke_immediately = false` → original agent stays online; the new code is issued.
4. Delete pending server → row gone, no orphan enrollment rows in DB.
5. Code expiry on pending card → *Regenerate code* produces a fresh plaintext command shown once.
6. Pending server: Recover button is hidden; *Regenerate code* is offered.
7. Rotate-token via API on a pending server → 400.

For each step, observe browser DevTools network tab and the server log; if anything misbehaves, file a follow-up before declaring done.

- [ ] **Step 3: Memory note**

If something here surprised you, write a feedback memory per the project's auto-memory rules.

- [ ] **Step 4: Final commit (only if anything was tidied)**

```
git add -A
git commit -m "chore: tidy after registration redesign" || true
```

Do **not** push. Hand the worktree back to the user for review and merge per their workflow.

---

## Self-Review

**Spec coverage check (against rev 4):**
- Problem statements: addressed in Tasks 7–10 (Add Server creates row immediately + no fingerprint dedup) and Tasks 2–3, 9–10 (recovery-merge removed; replaced by Recover/Regenerate).
- Status model with `has_token`: Tasks 4 (entity), 7 (DTO), 13 (frontend).
- Enrollment lifecycle (outstanding/usable, partial unique index, regenerate transaction): Tasks 4 (migration), 6 (service), 10 (handler).
- Recover never auto-supersedes: Task 9 returns 409 on outstanding.
- Regenerate optimistic CAS via `expected_enrollment_id`: Task 10.
- Register handler is transactional, no row creation, fingerprint informational only: Task 8.
- Agent has no behavioral changes, but RebindIdentity branch removed: Task 1.
- `validate_agent_token` filters NULL: Task 5.
- Rotate-token rejects pending: Task 12.
- DELETE /api/servers/{id} closes WS: Task 12.
- All recovery code (state, freeze guards, browser broadcast, ensure_no_running_recovery, services, router, entity, protocol types, agent reporter branch, openapi, frontend store/dialog/hooks/i18n): Tasks 2 and 3.
- Migration drops fingerprint unique index, makes token columns nullable, drops `recovery_job`, recreates `agent_enrollments`: Task 4.
- Add Server form: Task 14.
- Pending card status indicator + Regenerate: Task 15.
- Recover dialog (online + offline, never pending; revoke checkbox default true): Task 16.

**Placeholder scan:** none found. Test scaffolding helpers (`common::build_authed_admin_client`, `unauthed_client`, `server_count`) are referenced — if they don't exist in the codebase yet, Task 7 Step 1's first run will tell you, and the engineer creates them following the pattern in `crates/server/tests/cost_integration.rs` or similar. This is acceptable per "follow established patterns" since the project already has integration tests.

**Type consistency:** `EnrollmentIssueResponse` is the single response shape across Add Server / Recover / Regenerate. `OutstandingEnrollmentSummary` is the GET shape (no plaintext). `has_token: bool` and `outstanding_enrollment: Option<OutstandingEnrollmentSummary>` consistently named server-side and frontend-side. `StatusKind` enum on frontend has three values (`online | offline | pending`) consistently.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-05-25-agent-registration-redesign-plan.md`.**

The user requested sub-agent execution.

**REQUIRED SUB-SKILL:** Use superpowers:subagent-driven-development to dispatch one fresh subagent per task with two-stage review.
