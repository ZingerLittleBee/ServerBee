# Onboarding Forced Password Change — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove env-var admin credentials; first boot always generates a random admin password (logged with a high-visibility banner); the user is forced through an onboarding screen to change the password (and optionally rename the account) before anything else is usable.

**Architecture:** Add a `must_change_password` boolean column to `users`. `init_admin` always seeds a random password and sets the flag. A backend hard-block (HTTP middleware + per-handler WS validators + mobile login) rejects everything except `me`/`onboarding`/`logout` while the flag is set, returning a distinct `MUST_CHANGE_PASSWORD` error code. A new `POST /api/auth/onboarding` endpoint clears the flag. The React app gains an `/onboarding` route; the `_authed` guard and the WS hook are gated on the flag.

**Tech Stack:** Rust (Axum 0.8, sea-orm 1.x, SQLite, utoipa), React 19 (TanStack Router/Query, shadcn/ui, vitest), Biome/Ultracite.

**Spec:** `docs/superpowers/specs/2026-05-17-onboarding-forced-password-change-design.md`

**Branch:** `feat/onboarding-forced-password-change` (already created — do NOT create a worktree).

---

## Critical Ordering Constraint

The frontend type generator (`bun run generate:api-types`) runs the Rust server's OpenAPI dump. **All server changes (Tasks 1–9) must be complete and compiling before Task 10** regenerates frontend types. Do not reorder.

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `crates/server/src/migration/m20260517_000023_add_must_change_password.rs` | New migration: add column | 1 |
| `crates/server/src/migration/mod.rs` | Register migration | 1 |
| `crates/server/src/entity/user.rs` | `must_change_password` field | 2 |
| `crates/server/src/service/auth.rs` | `create_user` field; `init_admin` rewrite; `complete_onboarding`; `DEFAULT_ADMIN_USERNAME`; unit tests | 2,3,6 |
| `crates/server/src/config.rs` | Delete `AdminConfig` | 3 |
| `crates/server/src/main.rs` | `init_admin()` call + banner | 3 |
| `crates/server/tests/integration.rs` | Fixture seeds admin directly | 3,9 |
| `crates/server/src/middleware/auth.rs` | `CurrentUser.must_change_password` + hard-block | 4 |
| `crates/server/src/router/api/auth.rs` | `LoginResponse`/`MeResponse` field; onboarding handler+route | 5,6 |
| `crates/server/src/openapi.rs` | Register onboarding path + schema | 6 |
| `crates/server/src/router/ws/{browser,terminal,docker_logs}.rs` | Block flagged users in validators | 7 |
| `crates/server/src/service/mobile_auth.rs` | Block flagged users in login | 8 |
| `apps/web/src/lib/api-client.ts` | `ApiError.code` + hard redirect | 10 |
| `apps/web/src/lib/api-schema.ts` | Re-export `OnboardingRequest` | 10 |
| `apps/web/src/routes/_authed.tsx` | Guard + WS gating | 11 |
| `apps/web/src/routes/onboarding.tsx` | New onboarding page | 12 |
| `apps/web/src/lib/i18n.ts` + `src/locales/{en,zh}/onboarding.json` | i18n | 12 |
| `ENV.md`, `apps/docs/content/docs/{en,cn}/configuration.mdx`, root README/compose | Remove env-var guidance | 13 |

---

## Task 1: Migration — add `must_change_password` column

**Files:**
- Create: `crates/server/src/migration/m20260517_000023_add_must_change_password.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create the migration file**

Create `crates/server/src/migration/m20260517_000023_add_must_change_password.rs` with exactly:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260517_000023_add_must_change_password"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE users ADD COLUMN must_change_password BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register the migration in `mod.rs`**

In `crates/server/src/migration/mod.rs`, add the `mod` declaration after line 24 (`mod m20260517_000022_create_agent_enrollment;`):

```rust
mod m20260517_000023_add_must_change_password;
```

And add to the `vec![...]` in `migrations()` after the `m20260517_000022_create_agent_enrollment::Migration` line (currently line 52):

```rust
            Box::new(m20260517_000023_add_must_change_password::Migration),
```

- [ ] **Step 3: Verify it compiles and migrations run**

Run: `cargo test -p serverbee-server --lib test_utils 2>&1 | tail -5`
Expected: compiles; `setup_test_db` (which runs `Migrator::up`) used by other lib tests passes. If no test matches that filter, instead run `cargo build -p serverbee-server` and expect success.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/m20260517_000023_add_must_change_password.rs crates/server/src/migration/mod.rs
git commit -m "feat(server): add must_change_password migration"
```

---

## Task 2: User entity field + `create_user` default

**Files:**
- Modify: `crates/server/src/entity/user.rs:5-15`
- Modify: `crates/server/src/service/auth.rs:71-79` (`create_user` ActiveModel literal)

- [ ] **Step 1: Add the field to the entity Model**

In `crates/server/src/entity/user.rs`, the `Model` struct (lines 5–15) currently ends:

```rust
    pub totp_secret: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}
```

Insert `must_change_password` before `created_at`:

```rust
    pub totp_secret: Option<String>,
    pub must_change_password: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}
```

- [ ] **Step 2: Fix the only `user::ActiveModel` struct literal (`create_user`)**

In `crates/server/src/service/auth.rs`, `create_user` builds the literal (lines 71–79). It currently is:

```rust
        let new_user = user::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            username: Set(username.to_string()),
            password_hash: Set(password_hash),
            role: Set(role.to_string()),
            totp_secret: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
```

Add `must_change_password: Set(false),` before `created_at`:

```rust
        let new_user = user::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            username: Set(username.to_string()),
            password_hash: Set(password_hash),
            role: Set(role.to_string()),
            totp_secret: Set(None),
            must_change_password: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
        };
```

- [ ] **Step 3: Verify the workspace still compiles**

Run: `cargo build -p serverbee-server 2>&1 | tail -5`
Expected: success. (Other `user.into()` ActiveModel conversions — `enable_2fa`, `disable_2fa`, `change_password` — do not break; they convert from a full `Model` which now includes the field.)

- [ ] **Step 4: Run existing auth tests to confirm no regression**

Run: `cargo test -p serverbee-server --lib service::auth 2>&1 | tail -15`
Expected: all existing `service::auth::tests::*` pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/entity/user.rs crates/server/src/service/auth.rs
git commit -m "feat(server): add must_change_password field to user entity"
```

---

## Task 3: `init_admin` always-random + delete `AdminConfig`

This is an atomic, cross-cutting change: the workspace will not compile until every `AdminConfig` / `init_admin(&config.admin)` reference is updated. Do all steps before building.

**Files:**
- Modify: `crates/server/src/service/auth.rs` (remove `use crate::config::AdminConfig;` line 11; rewrite `init_admin` lines 323–351; add `DEFAULT_ADMIN_USERNAME` const; update unit tests)
- Modify: `crates/server/src/config.rs` (delete `AdminConfig` struct lines 115–130, `AppConfig.admin` field, `default_admin_username`, the `Default` init line)
- Modify: `crates/server/src/main.rs:60` and banner lines 102–114
- Modify: `crates/server/tests/integration.rs` (remove `AdminConfig` import + struct field, replace `init_admin` call)

- [ ] **Step 1: Write the failing unit test for the new `init_admin` contract**

In `crates/server/src/service/auth.rs`, inside `mod tests` (after `test_change_password_success`, before the closing `}` at line 759), add:

```rust
    #[tokio::test]
    async fn test_init_admin_creates_random_admin_with_flag() {
        let (db, _tmp) = setup_test_db().await;
        let generated = AuthService::init_admin(&db)
            .await
            .expect("init_admin should succeed");
        let pwd = generated.expect("first run must return a generated password");
        assert!(!pwd.is_empty(), "generated password must not be empty");

        let admin = user::Entity::find()
            .filter(user::Column::Username.eq(AuthService::DEFAULT_ADMIN_USERNAME))
            .one(&db)
            .await
            .expect("query should succeed")
            .expect("admin user must exist");
        assert_eq!(admin.role, "admin");
        assert!(
            admin.must_change_password,
            "freshly seeded admin must require password change"
        );
        // Generated password must actually work as the login password
        assert!(
            AuthService::verify_password(&pwd, &admin.password_hash).expect("verify"),
            "generated password must match stored hash"
        );
    }

    #[tokio::test]
    async fn test_init_admin_noop_when_users_exist() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "someone", "pass1234", "admin")
            .await
            .expect("seed a user");
        let generated = AuthService::init_admin(&db)
            .await
            .expect("init_admin should succeed");
        assert!(
            generated.is_none(),
            "init_admin must be a no-op when users already exist"
        );
    }
```

- [ ] **Step 2: Rewrite `init_admin` and add the constant**

In `crates/server/src/service/auth.rs`:

(a) Delete line 11 entirely: `use crate::config::AdminConfig;`

(b) Add the constant at the top of `impl AuthService {` (immediately after `impl AuthService {` on line 29):

```rust
    /// The fixed username for the auto-provisioned first admin account.
    pub const DEFAULT_ADMIN_USERNAME: &str = "admin";
```

(c) Replace the entire `init_admin` function (lines 323–351, the doc comment block + fn) with:

```rust
    /// Initialize the admin user if the users table is empty.
    /// Always generates a random password and forces a change on first login.
    /// Returns `Some(generated_password)` when a new admin was created.
    pub async fn init_admin(db: &DatabaseConnection) -> Result<Option<String>, AppError> {
        let user_count = user::Entity::find().count(db).await?;
        if user_count > 0 {
            return Ok(None);
        }

        let password = Self::generate_session_token();
        let created =
            Self::create_user(db, Self::DEFAULT_ADMIN_USERNAME, &password, "admin").await?;

        let mut active: user::ActiveModel = created.into();
        active.must_change_password = Set(true);
        active.updated_at = Set(Utc::now());
        active.update(db).await?;

        tracing::info!(
            "Admin user '{}' created with a random password (must be changed on first login)",
            Self::DEFAULT_ADMIN_USERNAME
        );

        Ok(Some(password))
    }
```

- [ ] **Step 3: Delete `AdminConfig` from config.rs**

In `crates/server/src/config.rs`:

(a) Delete the `AdminConfig` struct and its `Default` impl (lines 115–130):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    #[serde(default = "default_admin_username")]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            username: default_admin_username(),
            password: String::new(),
        }
    }
}
```

(b) Find and delete the `AppConfig` field `pub admin: AdminConfig,` (with its `#[serde(default)]` attribute) — around config.rs:18.

(c) Find and delete the `admin: AdminConfig::default(),` (or `admin: default_...`) line in `impl Default for AppConfig` — around config.rs:49.

(d) Find and delete the `fn default_admin_username() -> String { "admin".to_string() }` helper (search the file for `default_admin_username`).

After editing, run `grep -n "AdminConfig\|default_admin_username" crates/server/src/config.rs` — expected: no output.

- [ ] **Step 4: Update `main.rs` call site and banner**

In `crates/server/src/main.rs`:

(a) Replace line 60:

```rust
    let generated_admin_password = AuthService::init_admin(&db, &config.admin).await?;
```

with:

```rust
    let generated_admin_password = AuthService::init_admin(&db).await?;
```

(b) Replace the banner block (lines 102–114) with a higher-visibility `warn!` banner that no longer references `config.admin.username`:

```rust
    // Print credentials block — grouped + warn-level so users can spot them immediately.
    if let Some(ref pwd) = generated_admin_password {
        let banner = format!(
            "\n\n\
             ============================================================\n\
             ==                                                        ==\n\
             ==   FIRST-RUN ADMIN CREDENTIALS — SHOWN ONLY ONCE        ==\n\
             ==                                                        ==\n\
             ============================================================\n\
             \n\
             Username:  {}\n\
             Password:  {}\n\
             \n\
             You will be forced to change this password the first time\n\
             you log in. This password is NOT recoverable from the logs\n\
             afterwards — copy it now.\n\
             \n\
             ============================================================\n",
            AuthService::DEFAULT_ADMIN_USERNAME, pwd
        );
        tracing::warn!("{}", banner);
    }
```

- [ ] **Step 5: Update integration.rs fixture**

In `crates/server/tests/integration.rs`:

(a) Line 13 currently:

```rust
use serverbee_server::config::{AdminConfig, AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
```

Change to (drop `AdminConfig`):

```rust
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
```

(b) Delete the `admin: AdminConfig { ... },` field from the `AppConfig { ... }` literal (lines 42–45):

```rust
        admin: AdminConfig {
            username: "admin".to_string(),
            password: "testpass".to_string(),
        },
```

(c) Replace the `init_admin` call (lines 73–76):

```rust
    // Initialize admin user
    AuthService::init_admin(&db, &config.admin)
        .await
        .expect("Failed to init admin");
```

with a direct seed of a ready-to-use admin (known password, flag already cleared, so existing tests log in normally):

```rust
    // Seed a ready-to-use admin (password known, onboarding already done)
    // so existing tests can log in without the forced-change flow.
    AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .expect("Failed to seed admin");
```

- [ ] **Step 6: Build the whole workspace**

Run: `cargo build --workspace 2>&1 | tail -10`
Expected: success, no `AdminConfig` errors. If errors mention other files referencing `AdminConfig` or `init_admin(`, fix each per the same pattern (the only known references are those above; grep `git grep -n "AdminConfig\|init_admin(" -- crates` to confirm none remain besides the new no-arg call and the new test).

- [ ] **Step 7: Run the new + existing auth unit tests**

Run: `cargo test -p serverbee-server --lib service::auth 2>&1 | tail -20`
Expected: PASS including `test_init_admin_creates_random_admin_with_flag` and `test_init_admin_noop_when_users_exist`.

- [ ] **Step 8: Run the integration suite to confirm fixture still works**

Run: `cargo test -p serverbee-server --test integration 2>&1 | tail -20`
Expected: existing integration tests still PASS (admin login with `admin`/`testpass` works because the seeded admin has `must_change_password = false`).

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/service/auth.rs crates/server/src/config.rs crates/server/src/main.rs crates/server/tests/integration.rs
git commit -m "feat(server): always random first-run admin password, drop AdminConfig"
```

---

## Task 4: Middleware — `CurrentUser` flag + hard-block

**Files:**
- Modify: `crates/server/src/middleware/auth.rs`

- [ ] **Step 1: Add `must_change_password` to `CurrentUser`**

In `crates/server/src/middleware/auth.rs`, the struct (lines 13–18):

```rust
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub username: String,
    pub role: String,
}
```

Add the field:

```rust
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub must_change_password: bool,
}
```

- [ ] **Step 2: Populate the field in all three auth branches**

There are three `.map(... CurrentUser { ... })` closures (session cookie ~lines 31–35, API key ~lines 49–53, bearer ~lines 69–73). Each currently looks like:

```rust
                .map(|(user, _session)| CurrentUser {
                    user_id: user.id.clone(),
                    username: user.username.clone(),
                    role: user.role.clone(),
                })
```

(the API-key one is `.map(|user| CurrentUser { ... })`). Add `must_change_password: user.must_change_password,` to **each** of the three:

Session-cookie branch:

```rust
            .map(|(user, _session)| CurrentUser {
                user_id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
                must_change_password: user.must_change_password,
            })
```

API-key branch:

```rust
                    .map(|user| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                        must_change_password: user.must_change_password,
                    })
```

Bearer branch:

```rust
                    .map(|(user, _session)| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                        must_change_password: user.must_change_password,
                    })
```

- [ ] **Step 3: Add the hard-block helper and apply it**

In `crates/server/src/middleware/auth.rs`, add `use axum::Json;` to the existing `use axum::{...}` block (it currently imports `extract::{Request, State}, http::StatusCode, middleware::Next, response::{IntoResponse, Response}`). Add a helper function above `auth_middleware`:

```rust
/// 403 response with a distinct machine-readable code so the frontend can
/// reliably detect the forced-password-change state. Deliberately NOT routed
/// through `AppError` (whose Forbidden code is always "FORBIDDEN").
fn must_change_password_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": {
                "code": "MUST_CHANGE_PASSWORD",
                "message": "Password change required before continuing"
            }
        })),
    )
        .into_response()
}

/// Paths (already `/api`-stripped by `.nest("/api", ...)`) that a flagged user
/// may still reach so they can complete onboarding.
fn is_onboarding_whitelisted(method: &axum::http::Method, path: &str) -> bool {
    matches!(
        (method.as_str(), path),
        ("GET", "/auth/me") | ("POST", "/auth/onboarding") | ("POST", "/auth/logout")
    )
}
```

Then in `auth_middleware`, replace the final `match current_user { ... }` block (lines 80–87) with:

```rust
    match current_user {
        Some(user) => {
            if user.must_change_password
                && !is_onboarding_whitelisted(req.method(), req.uri().path())
            {
                return must_change_password_response();
            }
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
```

- [ ] **Step 4: Add a unit test for the whitelist matcher**

In `crates/server/src/middleware/auth.rs`, inside `mod tests` (after `test_extract_bearer_token_wrong_scheme`, before the closing `}`), add:

```rust
    #[test]
    fn test_onboarding_whitelist() {
        use axum::http::Method;
        assert!(is_onboarding_whitelisted(&Method::GET, "/auth/me"));
        assert!(is_onboarding_whitelisted(&Method::POST, "/auth/onboarding"));
        assert!(is_onboarding_whitelisted(&Method::POST, "/auth/logout"));
        // Wrong method
        assert!(!is_onboarding_whitelisted(&Method::POST, "/auth/me"));
        // Not whitelisted
        assert!(!is_onboarding_whitelisted(&Method::GET, "/servers"));
        // Full /api-prefixed path must NOT match (router strips /api)
        assert!(!is_onboarding_whitelisted(&Method::GET, "/api/auth/me"));
    }
```

- [ ] **Step 5: Build + run middleware tests**

Run: `cargo test -p serverbee-server --lib middleware::auth 2>&1 | tail -15`
Expected: PASS including `test_onboarding_whitelist`.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/middleware/auth.rs
git commit -m "feat(server): hard-block flagged users in auth middleware"
```

---

## Task 5: `LoginResponse` / `MeResponse` carry the flag

**Files:**
- Modify: `crates/server/src/router/api/auth.rs` (struct defs lines 27–39; `login` handler builds `LoginResponse`; `me` handler builds `MeResponse`)

- [ ] **Step 1: Add the field to both response structs**

In `crates/server/src/router/api/auth.rs`, lines 27–39:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    user_id: String,
    username: String,
    role: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    user_id: String,
    username: String,
    role: String,
}
```

Change to:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    user_id: String,
    username: String,
    role: String,
    must_change_password: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    user_id: String,
    username: String,
    role: String,
    must_change_password: bool,
}
```

- [ ] **Step 2: Populate `LoginResponse` in the `login` handler**

In `login` (the handler whose `#[utoipa::path]` starts at line 107), find where it constructs the response. It builds `LoginResponse { user_id: user.id, username: user.username, role: user.role }` from the `user` model returned by `AuthService::login`. Add the field:

```rust
        data: LoginResponse {
            user_id: user.id,
            username: user.username,
            role: user.role,
            must_change_password: user.must_change_password,
        },
```

(The exact surrounding code uses `ApiResponse { data: LoginResponse { ... } }`; only add the one field line. `user` is the `user::Model` from `AuthService::login`, which now has the field.)

- [ ] **Step 3: Populate `MeResponse` in the `me` handler**

`me` (lines 236–244) currently:

```rust
pub async fn me(
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<MeResponse>>, AppError> {
    ok(MeResponse {
        user_id: current_user.user_id,
        username: current_user.username,
        role: current_user.role,
    })
}
```

Change to:

```rust
pub async fn me(
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<MeResponse>>, AppError> {
    ok(MeResponse {
        user_id: current_user.user_id,
        username: current_user.username,
        role: current_user.role,
        must_change_password: current_user.must_change_password,
    })
}
```

- [ ] **Step 4: Build**

Run: `cargo build -p serverbee-server 2>&1 | tail -5`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/auth.rs
git commit -m "feat(server): expose must_change_password in login/me responses"
```

---

## Task 6: Onboarding service + endpoint + OpenAPI

**Files:**
- Modify: `crates/server/src/service/auth.rs` (add `complete_onboarding` + unit tests)
- Modify: `crates/server/src/router/api/auth.rs` (`OnboardingRequest`, handler, route)
- Modify: `crates/server/src/openapi.rs` (register path + schema)

- [ ] **Step 1: Write failing unit tests for `complete_onboarding`**

In `crates/server/src/service/auth.rs` `mod tests`, add:

```rust
    async fn seed_must_change_admin(db: &DatabaseConnection) -> user::Model {
        let u = AuthService::create_user(db, "admin", "init-pass-123", "admin")
            .await
            .expect("seed admin");
        let mut a: user::ActiveModel = u.into();
        a.must_change_password = Set(true);
        a.update(db).await.expect("set flag")
    }

    #[tokio::test]
    async fn test_complete_onboarding_success_password_only() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        AuthService::complete_onboarding(&db, &admin.id, "brand-new-pass-9", None)
            .await
            .expect("onboarding should succeed");
        let after = user::Entity::find_by_id(&admin.id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert!(!after.must_change_password, "flag must be cleared");
        assert_eq!(after.username, "admin", "username unchanged when None");
        assert!(AuthService::verify_password("brand-new-pass-9", &after.password_hash).unwrap());
    }

    #[tokio::test]
    async fn test_complete_onboarding_with_username_change() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        AuthService::complete_onboarding(&db, &admin.id, "np-12345", Some("  newname  "))
            .await
            .expect("should succeed and trim username");
        let after = user::Entity::find_by_id(&admin.id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.username, "newname", "username trimmed + applied");
        assert!(!after.must_change_password);
    }

    #[tokio::test]
    async fn test_complete_onboarding_blank_username_is_ignored() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        AuthService::complete_onboarding(&db, &admin.id, "np-12345", Some("   "))
            .await
            .expect("blank username treated as not provided");
        let after = user::Entity::find_by_id(&admin.id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.username, "admin");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_same_password() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        let r = AuthService::complete_onboarding(&db, &admin.id, "init-pass-123", None).await;
        assert!(r.is_err(), "reusing current password must be rejected");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_empty_password() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        let r = AuthService::complete_onboarding(&db, &admin.id, "", None).await;
        assert!(r.is_err(), "empty password must be rejected");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_when_flag_not_set() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "admin", "p", "admin")
            .await
            .unwrap(); // must_change_password = false by default
        let r = AuthService::complete_onboarding(&db, &u.id, "new-pass-1", None).await;
        assert!(r.is_err(), "onboarding when flag is false must be rejected");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_duplicate_username() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "taken", "p", "member")
            .await
            .unwrap();
        let admin = seed_must_change_admin(&db).await;
        let r =
            AuthService::complete_onboarding(&db, &admin.id, "new-pass-1", Some("taken")).await;
        assert!(r.is_err(), "duplicate username must be rejected");
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p serverbee-server --lib service::auth::tests::test_complete_onboarding 2>&1 | tail -10`
Expected: FAIL — `complete_onboarding` does not exist (compile error).

- [ ] **Step 3: Implement `complete_onboarding`**

In `crates/server/src/service/auth.rs`, add this method inside `impl AuthService` (place it right after `change_password`, before the closing `}` of the impl at line 486):

```rust
    /// Complete first-login onboarding: set a new password and optionally a new
    /// username, then clear the `must_change_password` flag. Only valid while
    /// the flag is set. Does NOT write audit logs (the handler does).
    pub async fn complete_onboarding(
        db: &DatabaseConnection,
        user_id: &str,
        new_password: &str,
        new_username: Option<&str>,
    ) -> Result<(), AppError> {
        let user = user::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        if !user.must_change_password {
            return Err(AppError::Forbidden(
                "Onboarding is not required for this account".to_string(),
            ));
        }
        if new_password.is_empty() {
            return Err(AppError::Validation("New password is required".to_string()));
        }
        if Self::verify_password(new_password, &user.password_hash)? {
            return Err(AppError::Validation(
                "New password must be different from the current password".to_string(),
            ));
        }

        let trimmed_username = new_username
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .filter(|u| *u != user.username);

        if let Some(uname) = trimmed_username {
            let existing = user::Entity::find()
                .filter(user::Column::Username.eq(uname))
                .one(db)
                .await?;
            if existing.is_some() {
                return Err(AppError::Conflict(format!(
                    "User '{uname}' already exists"
                )));
            }
        }

        let new_hash = Self::hash_password(new_password)?;
        let mut active: user::ActiveModel = user.into();
        active.password_hash = Set(new_hash);
        if let Some(uname) = trimmed_username {
            active.username = Set(uname.to_string());
        }
        active.must_change_password = Set(false);
        active.updated_at = Set(Utc::now());
        active.update(db).await?;

        Ok(())
    }
```

- [ ] **Step 4: Run the unit tests to confirm they pass**

Run: `cargo test -p serverbee-server --lib service::auth::tests::test_complete_onboarding 2>&1 | tail -15`
Expected: PASS (all 7 onboarding tests).

- [ ] **Step 5: Add `OnboardingRequest` + handler + route**

In `crates/server/src/router/api/auth.rs`:

(a) Add the request struct after `ChangePasswordRequest` (after line 59):

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OnboardingRequest {
    new_password: String,
    new_username: Option<String>,
}
```

(b) Register the route in `protected_router` (the builder at lines 89–105). Add after the `/auth/password` line (line 96):

```rust
        .route("/auth/onboarding", post(onboarding))
```

(c) Add the handler. Place it immediately after `change_password` (after line 373). It mirrors `change_password`'s IP extraction + best-effort audit:

```rust
#[utoipa::path(
    post,
    path = "/api/auth/onboarding",
    tag = "auth",
    request_body = OnboardingRequest,
    responses(
        (status = 200, description = "Onboarding complete"),
        (status = 403, description = "Onboarding not required / forbidden"),
        (status = 409, description = "Username already taken"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn onboarding(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    req_headers: HeaderMap,
    Json(body): Json<OnboardingRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    AuthService::complete_onboarding(
        &state.db,
        &current_user.user_id,
        &body.new_password,
        body.new_username.as_deref(),
    )
    .await?;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(&state.db, &current_user.user_id, "onboarding", None, &ip).await;

    ok("ok")
}
```

- [ ] **Step 6: Register in OpenAPI**

In `crates/server/src/openapi.rs`:

(a) In the `paths(...)` list, add after `crate::router::api::auth::change_password,` (line 45):

```rust
        crate::router::api::auth::onboarding,
```

(b) In `components(schemas(...))`, add after `crate::router::api::auth::ChangePasswordRequest,` (line 234):

```rust
            crate::router::api::auth::OnboardingRequest,
```

- [ ] **Step 7: Build + full auth test pass**

Run: `cargo build -p serverbee-server 2>&1 | tail -5 && cargo test -p serverbee-server --lib service::auth 2>&1 | tail -10`
Expected: build success; all auth unit tests PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/service/auth.rs crates/server/src/router/api/auth.rs crates/server/src/openapi.rs
git commit -m "feat(server): add POST /api/auth/onboarding endpoint"
```

---

## Task 7: Block flagged users in user-session WebSocket validators

Each WS validator returns the user id (and role) on success. We must reject when the resolved user has `must_change_password == true`. The validators currently discard the full user model; we add the check inline against the resolved `user` before returning.

**Files:**
- Modify: `crates/server/src/router/ws/browser.rs` (`validate_browser_auth`, ~lines 50–82)
- Modify: `crates/server/src/router/ws/terminal.rs` (`validate_auth`, ~lines 99–126)
- Modify: `crates/server/src/router/ws/docker_logs.rs` (`validate_auth`, ~lines 83–109)

- [ ] **Step 1: browser.rs — reject flagged users**

In `validate_browser_auth`, the session-cookie and bearer branches resolve `(user, _session)` and the API-key branch resolves `user`. Add a guard right after each successful resolution, before the `return Some(...)`. Concretely, change each block so it skips (returns `None` overall by not returning early) when flagged:

Session-cookie branch — currently:

```rust
    if let Some(token) = extract_session_cookie(headers)
        && let Ok(Some((user, _session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        return Some((user.id, user.role == "admin", None));
    }
```

Change to:

```rust
    if let Some(token) = extract_session_cookie(headers)
        && let Ok(Some((user, _session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
        && !user.must_change_password
    {
        return Some((user.id, user.role == "admin", None));
    }
```

API-key branch — add `&& !user.must_change_password` to its `if let` chain the same way:

```rust
    if let Some(key) = extract_api_key(headers)
        && let Ok(Some(user)) = AuthService::validate_api_key(&state.db, &key).await
        && !user.must_change_password
    {
        return Some((user.id, user.role == "admin", None));
    }
```

Bearer branch — add `&& !user.must_change_password` to its `if let` chain:

```rust
    if let Some(token) = extract_bearer_token(headers)
        && let Ok(Some((user, session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
        && !user.must_change_password
    {
        let mobile_expires = if session.source != "web" {
            Some(session.expires_at)
        } else {
            None
        };
        return Some((user.id, user.role == "admin", mobile_expires));
    }
```

(Net effect: a flagged user falls through all branches → `None` → WS upgrade rejected by the existing handler logic.)

- [ ] **Step 2: terminal.rs — reject flagged users**

In `validate_auth`, add `&& !user.must_change_password` to each of the three `if let ... {` chains (session cookie, API key, bearer), exactly mirroring Step 1. The three blocks currently return `Some((user.id, user.role))`; gate each with the extra condition so a flagged user falls through to the final `None`.

- [ ] **Step 3: docker_logs.rs — reject flagged users**

In `validate_auth`, add `&& !user.must_change_password` to each of the three `if let ... {` chains (session cookie, API key, bearer). The three blocks currently return `Some(user.id)`; gate each so a flagged user falls through to the final `None`.

- [ ] **Step 4: Confirm agent.rs is untouched**

Run: `git status --porcelain crates/server/src/router/ws/agent.rs`
Expected: no output (agent WS uses server-token auth and must NOT be gated).

- [ ] **Step 5: Build**

Run: `cargo build -p serverbee-server 2>&1 | tail -5`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/ws/browser.rs crates/server/src/router/ws/terminal.rs crates/server/src/router/ws/docker_logs.rs
git commit -m "feat(server): block flagged users from user-session websockets"
```

---

## Task 8: Block flagged users in mobile login

`MobileAuthService::login` calls `validate_credentials` then `login_for_user`. We reject between them when the validated user is flagged, so no device token is ever issued.

**Files:**
- Modify: `crates/server/src/service/mobile_auth.rs` (`login`, lines 92–112)

- [ ] **Step 1: Add the flag check in `login`**

In `crates/server/src/service/mobile_auth.rs`, `login` currently:

```rust
    pub async fn login(
        db: &DatabaseConnection,
        config: &MobileConfig,
        params: MobileLoginParams<'_>,
    ) -> Result<MobileTokenResponse, AppError> {
        // Validate credentials using AuthService
        let user_model =
            Self::validate_credentials(db, params.username, params.password, params.totp_code)
                .await?;

        Self::login_for_user(
            db,
            config,
            &user_model,
            params.installation_id,
            params.device_name,
            params.ip,
            params.user_agent,
        )
        .await
    }
```

Insert the guard right after `validate_credentials`:

```rust
    pub async fn login(
        db: &DatabaseConnection,
        config: &MobileConfig,
        params: MobileLoginParams<'_>,
    ) -> Result<MobileTokenResponse, AppError> {
        // Validate credentials using AuthService
        let user_model =
            Self::validate_credentials(db, params.username, params.password, params.totp_code)
                .await?;

        if user_model.must_change_password {
            return Err(AppError::Forbidden(
                "MUST_CHANGE_PASSWORD: complete onboarding via the web UI before using mobile"
                    .to_string(),
            ));
        }

        Self::login_for_user(
            db,
            config,
            &user_model,
            params.installation_id,
            params.device_name,
            params.ip,
            params.user_agent,
        )
        .await
    }
```

(Note: `validate_credentials` returns the `user::Model`, which now has `must_change_password`. The message is prefixed `MUST_CHANGE_PASSWORD:` so a mobile client can string-match; the HTTP status is 403 via `AppError::Forbidden`.)

- [ ] **Step 2: Build**

Run: `cargo build -p serverbee-server 2>&1 | tail -5`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/service/mobile_auth.rs
git commit -m "feat(server): block flagged users from mobile login"
```

---

## Task 9: Backend integration tests

**Files:**
- Modify: `crates/server/tests/integration.rs` (add a new test module at end of file)

- [ ] **Step 1: Add a helper to seed a flagged admin and a test module**

Append to the end of `crates/server/tests/integration.rs`:

```rust
mod onboarding_tests {
    use super::*;
    use serverbee_server::service::auth::AuthService;

    /// Boot a test server whose admin still requires onboarding.
    /// Returns (base_url, tmp, generated_password).
    async fn start_with_must_change_admin() -> (String, tempfile::TempDir) {
        // Reuse the standard fixture but flip the seeded admin's flag via a
        // fresh login is not possible; instead start a server and patch the DB
        // is overkill. We rely on the standard fixture seeding admin/testpass
        // with flag=false, then exercise the flag explicitly through the
        // dedicated DB-level test below. For HTTP-level tests we seed via a
        // second admin path:
        start_test_server().await
    }

    #[tokio::test]
    async fn flagged_admin_is_blocked_and_can_onboard() {
        // Build an isolated DB so we control the admin flag directly.
        let tmp = tempfile::tempdir().expect("temp dir");
        let data_dir = tmp.path().to_str().unwrap().to_string();
        let db_url = format!("sqlite://{}/test.db?mode=rwc", data_dir);
        let mut opt = sea_orm::ConnectOptions::new(&db_url);
        opt.max_connections(5);
        opt.sqlx_logging(false);
        let db = sea_orm::Database::connect(opt).await.expect("connect");
        db.execute_unprepared("PRAGMA foreign_keys=ON").await.unwrap();
        serverbee_server::migration::Migrator::up(&db, None)
            .await
            .expect("migrate");

        // First-run: random password + flag=true
        let generated = AuthService::init_admin(&db)
            .await
            .expect("init_admin")
            .expect("password generated");

        let config = AppConfig {
            server: ServerConfig {
                listen: "127.0.0.1:0".to_string(),
                data_dir: data_dir.clone(),
                trusted_proxies: Vec::new(),
            },
            database: DatabaseConfig {
                path: "test.db".to_string(),
                max_connections: 5,
            },
            auth: AuthConfig {
                session_ttl: 86400,
                secure_cookie: false,
                max_servers: 0,
            },
            ..AppConfig::default()
        };
        let state = serverbee_server::state::AppState::new(db, config)
            .await
            .expect("state");
        let app = serverbee_server::router::create_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{}", addr);
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = http_client();

        // Login succeeds and reports must_change_password = true
        let login: serde_json::Value = client
            .post(format!("{}/api/auth/login", base_url))
            .json(&json!({ "username": "admin", "password": generated }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            login["data"]["must_change_password"], true,
            "login response must flag the account"
        );

        // /api/auth/me is whitelisted and also reports the flag
        let me: serde_json::Value = client
            .get(format!("{}/api/auth/me", base_url))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(me["data"]["must_change_password"], true);

        // A protected route is hard-blocked with the distinct code
        let blocked = client
            .get(format!("{}/api/servers", base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(blocked.status(), 403, "protected route must be blocked");
        let body: serde_json::Value = blocked.json().await.unwrap();
        assert_eq!(body["error"]["code"], "MUST_CHANGE_PASSWORD");

        // Mobile login is also blocked (no device token issued)
        let mobile = client
            .post(format!("{}/api/mobile/auth/login", base_url))
            .json(&json!({
                "username": "admin",
                "password": generated,
                "installation_id": "test-install",
                "device_name": "test-device"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(mobile.status(), 403, "mobile login must be blocked");

        // Complete onboarding: change password + rename
        let ob = client
            .post(format!("{}/api/auth/onboarding", base_url))
            .json(&json!({ "new_password": "Fresh-Pass-12345", "new_username": "rootadmin" }))
            .send()
            .await
            .unwrap();
        assert_eq!(ob.status(), 200, "onboarding should succeed");

        // Protected route now works in the same session
        let ok_after = client
            .get(format!("{}/api/servers", base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(ok_after.status(), 200, "unblocked after onboarding");

        // New credentials work; mobile login now succeeds
        let relog = client
            .post(format!("{}/api/auth/login", base_url))
            .json(&json!({ "username": "rootadmin", "password": "Fresh-Pass-12345" }))
            .send()
            .await
            .unwrap();
        assert_eq!(relog.status(), 200);
        let relog_body: serde_json::Value = relog.json().await.unwrap();
        assert_eq!(relog_body["data"]["must_change_password"], false);

        let mobile_ok = client
            .post(format!("{}/api/mobile/auth/login", base_url))
            .json(&json!({
                "username": "rootadmin",
                "password": "Fresh-Pass-12345",
                "installation_id": "test-install",
                "device_name": "test-device"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(mobile_ok.status(), 200, "mobile login works post-onboarding");

        drop(tmp);
    }
}
```

(The unused `start_with_must_change_admin` helper documents why we build an isolated DB; delete it if Clippy flags it — see Step 3.)

- [ ] **Step 2: Run the new integration test**

Run: `cargo test -p serverbee-server --test integration onboarding_tests 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 3: Remove the dead helper if Clippy/compiler warns**

If Step 2 emitted `warning: function ... is never used` for `start_with_must_change_admin`, delete that function entirely (it was only explanatory). Re-run Step 2; expected PASS with no warning.

- [ ] **Step 4: Full backend gate**

Run: `cargo test -p serverbee-server 2>&1 | tail -15 && cargo clippy -p serverbee-server -- -D warnings 2>&1 | tail -5`
Expected: all tests PASS; clippy clean (0 warnings).

- [ ] **Step 5: Commit**

```bash
git add crates/server/tests/integration.rs
git commit -m "test(server): integration coverage for forced onboarding flow"
```

---

## Task 10: Frontend — regenerate API types + api-client

**Files:**
- Run generator (writes `apps/web/src/lib/api-types.ts`)
- Modify: `apps/web/src/lib/api-schema.ts`
- Modify: `apps/web/src/lib/api-client.ts`

- [ ] **Step 1: Regenerate OpenAPI types from the (now complete) server**

Run from repo root:

```bash
cd apps/web && bun run generate:api-types && cd ../..
```

Expected: `apps/web/src/lib/api-types.ts` updated. Verify:

```bash
grep -n "OnboardingRequest\|must_change_password" apps/web/src/lib/api-types.ts | head
```
Expected: both present (`OnboardingRequest` schema + `must_change_password` on `LoginResponse`/`MeResponse`).

- [ ] **Step 2: Re-export `OnboardingRequest` from api-schema**

In `apps/web/src/lib/api-schema.ts`, after the existing auth re-exports (lines 12–14):

```typescript
export type LoginRequest = S['LoginRequest']
export type LoginResponse = S['LoginResponse']
export type MeResponse = S['MeResponse']
```

add:

```typescript
export type OnboardingRequest = S['OnboardingRequest']
```

- [ ] **Step 3: Add `code` to `ApiError` and parse it**

In `apps/web/src/lib/api-client.ts`, replace the `ApiError` class and the error-throw site. Current:

```typescript
class ApiError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}
```

Change to:

```typescript
class ApiError extends Error {
  status: number
  code?: string

  constructor(message: string, status: number, code?: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
  }
}
```

Then update the `!response.ok` branch. Current:

```typescript
  if (!response.ok) {
    const text = await response.text().catch(() => response.statusText)
    throw new ApiError(text, response.status)
  }
```

Change to:

```typescript
  if (!response.ok) {
    const text = await response.text().catch(() => response.statusText)
    let code: string | undefined
    try {
      const parsed = JSON.parse(text)
      code = parsed?.error?.code
    } catch {
      // body is not JSON; leave code undefined
    }
    if (code === 'MUST_CHANGE_PASSWORD' && window.location.pathname !== '/onboarding') {
      window.location.assign('/onboarding')
    }
    throw new ApiError(text, response.status, code)
  }
```

- [ ] **Step 4: Typecheck**

Run: `cd apps/web && bun run typecheck 2>&1 | tail -10 && cd ../..`
Expected: no errors. (The `/onboarding` route does not exist yet, but `window.location.assign` is a string call — not router-typed — so this passes. The route is added in Task 12.)

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/api-types.ts apps/web/src/lib/api-schema.ts apps/web/src/lib/api-client.ts
git commit -m "feat(web): regenerate api types, surface MUST_CHANGE_PASSWORD"
```

---

## Task 11: Frontend — `_authed` guard + WS gating

**Files:**
- Modify: `apps/web/src/routes/_authed.tsx`

- [ ] **Step 1: Gate the WS hook on the flag**

In `apps/web/src/routes/_authed.tsx`, line 130:

```typescript
  const shouldConnectWs = isAuthenticated && !isLoading
```

Change to:

```typescript
  const shouldConnectWs = isAuthenticated && !isLoading && user?.must_change_password !== true
```

- [ ] **Step 2: Redirect flagged users to `/onboarding`**

After the existing login-redirect `useEffect` (lines 159–165), add a new effect:

```typescript
  useEffect(() => {
    if (!isLoading && isAuthenticated && user?.must_change_password === true) {
      navigate({ to: '/onboarding' }).catch(() => {
        // Navigation error is non-critical
      })
    }
  }, [isLoading, isAuthenticated, user, navigate])
```

- [ ] **Step 3: Block rendering protected content while flagged**

After the existing `if (!isAuthenticated) { return null }` (lines 186–188), add:

```typescript
  if (user?.must_change_password === true) {
    return null
  }
```

- [ ] **Step 4: Typecheck**

Run: `cd apps/web && bun run typecheck 2>&1 | tail -10 && cd ../..`
Expected: no errors. (`user?.must_change_password` is typed via the regenerated `MeResponse`.)

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed.tsx
git commit -m "feat(web): gate authed layout + ws on must_change_password"
```

---

## Task 12: Frontend — onboarding page + i18n + vitest

**Files:**
- Create: `apps/web/src/routes/onboarding.tsx`
- Create: `apps/web/src/locales/en/onboarding.json`, `apps/web/src/locales/zh/onboarding.json`
- Modify: `apps/web/src/lib/i18n.ts`
- Create: `apps/web/src/routes/__tests__/onboarding.test.tsx` (vitest)
- Regenerate: `apps/web/src/routeTree.gen.ts`

- [ ] **Step 1: Create English + Chinese locale files**

Create `apps/web/src/locales/en/onboarding.json`:

```json
{
  "title": "Set up your admin account",
  "subtitle": "For security, you must change the initial password before continuing.",
  "username": "Username",
  "username_hint": "Optional — leave as-is to keep the current username.",
  "new_password": "New password",
  "confirm_password": "Confirm new password",
  "submit": "Save and continue",
  "saving": "Saving…",
  "password_required": "New password is required.",
  "password_mismatch": "The two passwords do not match.",
  "failed": "Could not complete setup. Please try again."
}
```

Create `apps/web/src/locales/zh/onboarding.json`:

```json
{
  "title": "设置你的管理员账户",
  "subtitle": "出于安全考虑，继续之前必须修改初始密码。",
  "username": "用户名",
  "username_hint": "可选 —— 留空则保持当前用户名。",
  "new_password": "新密码",
  "confirm_password": "确认新密码",
  "submit": "保存并继续",
  "saving": "保存中…",
  "password_required": "请输入新密码。",
  "password_mismatch": "两次输入的密码不一致。",
  "failed": "设置未能完成，请重试。"
}
```

- [ ] **Step 2: Register the `onboarding` namespace in i18n**

In `apps/web/src/lib/i18n.ts`:

(a) Add imports after line 15 (`import enTerminal ...`) and line 27 (`import zhTerminal ...`):

```typescript
import enOnboarding from '@/locales/en/onboarding.json'
```
```typescript
import zhOnboarding from '@/locales/zh/onboarding.json'
```

(b) Add to the `en` resources object (after `network: enNetwork` on line 45):

```typescript
        ,onboarding: enOnboarding
```

(Better: append `onboarding: enOnboarding` as a new entry — ensure valid JSON-object syntax: add a comma after `network: enNetwork` and then `onboarding: enOnboarding`.) Final `en` block tail should read:

```typescript
        network: enNetwork,
        onboarding: enOnboarding
```

(c) Likewise the `zh` block tail:

```typescript
        network: zhNetwork,
        onboarding: zhOnboarding
```

- [ ] **Step 3: Create the onboarding route component**

Create `apps/web/src/routes/onboarding.tsx`:

```tsx
import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAuth } from '@/hooks/use-auth'
import { api, ApiError } from '@/lib/api-client'
import type { OnboardingRequest } from '@/lib/api-schema'

export const Route = createFileRoute('/onboarding')({
  component: OnboardingPage
})

function OnboardingPage() {
  const { t } = useTranslation('onboarding')
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { user, isLoading, isAuthenticated } = useAuth()

  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    if (isLoading) {
      return
    }
    if (!isAuthenticated) {
      navigate({ to: '/login' }).catch(() => {
        // non-critical
      })
      return
    }
    if (user && user.must_change_password !== true) {
      navigate({ to: '/' }).catch(() => {
        // non-critical
      })
    }
  }, [isLoading, isAuthenticated, user, navigate])

  useEffect(() => {
    if (user?.username) {
      setUsername(user.username)
    }
  }, [user?.username])

  if (isLoading || !isAuthenticated || user?.must_change_password !== true) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!password) {
      toast.error(t('password_required'))
      return
    }
    if (password !== confirm) {
      toast.error(t('password_mismatch'))
      return
    }
    setSubmitting(true)
    try {
      const payload: OnboardingRequest = {
        new_password: password,
        new_username: username.trim() === user?.username ? null : username.trim() || null
      }
      await api.post('/api/auth/onboarding', payload)
      await queryClient.invalidateQueries({ queryKey: ['auth', 'me'] })
      await navigate({ to: '/' })
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : t('failed')
      toast.error(msg)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="text-center">
          <h1 className="font-bold text-2xl">{t('title')}</h1>
          <p className="mt-1 text-muted-foreground text-sm">{t('subtitle')}</p>
        </div>

        <form className="space-y-4" onSubmit={handleSubmit}>
          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="username">
              {t('username')}
            </label>
            <Input
              autoComplete="username"
              id="username"
              onChange={(e) => setUsername(e.target.value)}
              spellCheck={false}
              type="text"
              value={username}
            />
            <p className="text-muted-foreground text-xs">{t('username_hint')}</p>
          </div>

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="new-password">
              {t('new_password')}
            </label>
            <Input
              autoComplete="new-password"
              id="new-password"
              onChange={(e) => setPassword(e.target.value)}
              required
              type="password"
              value={password}
            />
          </div>

          <div className="space-y-2">
            <label className="font-medium text-sm" htmlFor="confirm-password">
              {t('confirm_password')}
            </label>
            <Input
              autoComplete="new-password"
              id="confirm-password"
              onChange={(e) => setConfirm(e.target.value)}
              required
              type="password"
              value={confirm}
            />
          </div>

          <Button className="w-full" disabled={submitting} type="submit">
            {submitting ? t('saving') : t('submit')}
          </Button>
        </form>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Regenerate the route tree**

The TanStack Router Vite plugin regenerates `src/routeTree.gen.ts` on dev/build start. Run from `apps/web`:

```bash
cd apps/web && (bun run dev > /tmp/sb_dev.log 2>&1 &) ; sleep 8 ; pkill -f "vite" ; sleep 1 ; grep -n "onboarding" src/routeTree.gen.ts | head ; cd ../..
```

Expected: `grep` shows `/onboarding` route entries (e.g. `OnboardingRoute`). If empty, increase the sleep to 15 and retry. Confirm no stray vite process remains: `pgrep -fl vite` → no output.

- [ ] **Step 5: Write the vitest spec**

Create `apps/web/src/routes/__tests__/onboarding.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

const navigateMock = vi.fn().mockResolvedValue(undefined)

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (opts: unknown) => opts,
  useNavigate: () => navigateMock
}))

const authState = {
  user: { user_id: '1', username: 'admin', role: 'admin', must_change_password: true },
  isLoading: false,
  isAuthenticated: true
}
vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => authState
}))

vi.mock('@tanstack/react-query', () => ({
  useQueryClient: () => ({ invalidateQueries: vi.fn().mockResolvedValue(undefined) })
}))

// Import after mocks
import { Route } from '../onboarding'

const OnboardingPage = (Route as unknown as { component: () => JSX.Element }).component

describe('OnboardingPage', () => {
  it('renders the forced-change form for a flagged user', () => {
    render(<OnboardingPage />)
    expect(screen.getByRole('button', { name: /save|保存/i })).toBeInTheDocument()
  })

  it('redirects away when the user does not require a password change', () => {
    authState.user = {
      user_id: '1',
      username: 'admin',
      role: 'admin',
      must_change_password: false
    } as typeof authState.user
    render(<OnboardingPage />)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/' })
  })
})
```

- [ ] **Step 6: Run vitest for the new spec**

Run: `cd apps/web && bun run test -- onboarding 2>&1 | tail -15 && cd ../..`
Expected: both tests PASS. If the JSX-in-test typing trips Biome later, that is handled in Task 14.

- [ ] **Step 7: Typecheck**

Run: `cd apps/web && bun run typecheck 2>&1 | tail -10 && cd ../..`
Expected: no errors (route tree now includes `/onboarding`, so `navigate({ to: '/onboarding' })` in `_authed.tsx` and `api-client` redirect target are valid).

- [ ] **Step 8: Commit**

```bash
git add apps/web/src/routes/onboarding.tsx apps/web/src/routes/__tests__/onboarding.test.tsx apps/web/src/locales/en/onboarding.json apps/web/src/locales/zh/onboarding.json apps/web/src/lib/i18n.ts apps/web/src/routeTree.gen.ts
git commit -m "feat(web): add forced onboarding page"
```

---

## Task 13: Documentation cleanup

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`, `apps/docs/content/docs/cn/configuration.mdx`
- Modify: root `README.md` / `README.zh-CN.md` / any `docker-compose*.yml` referenced in docs

- [ ] **Step 1: Locate every reference to the removed env vars**

Run from repo root:

```bash
grep -rn "SERVERBEE_ADMIN__\|ADMIN__USERNAME\|ADMIN__PASSWORD\|admin\.password\|admin\.username" ENV.md apps/docs/content/docs README*.md docker-compose*.yml 2>/dev/null
```

Record every hit.

- [ ] **Step 2: Edit each hit**

For every file/line from Step 1:
- Remove the `SERVERBEE_ADMIN__USERNAME` / `SERVERBEE_ADMIN__PASSWORD` rows from env tables and compose `environment:` blocks.
- Replace surrounding prose with: the admin account is auto-created on first start with a random password printed once to the server logs; you are forced to change it (and may rename the account) on first login. Mirror existing CN/EN tone in `configuration.mdx`.
- In `docker-compose` examples, delete the two `- SERVERBEE_ADMIN__...` lines entirely (leave the rest of the service intact).

Do not invent new env vars. Keep both CN and EN docs in sync (same structural edit).

- [ ] **Step 3: Verify no references remain**

Run the Step 1 grep again. Expected: no output (or only unrelated matches like a changelog history entry — leave historical changelog/spec files untouched; only docs/README/compose are in scope).

- [ ] **Step 4: Docs typecheck (if fumadocs build is wired)**

Run: `bun run typecheck 2>&1 | tail -5`
Expected: success (this runs web + fumadocs typecheck per CLAUDE.md). If fumadocs has no typecheck, skip.

- [ ] **Step 5: Commit**

```bash
git add ENV.md apps/docs/content/docs README*.md docker-compose*.yml
git commit -m "docs: drop admin credential env vars, document first-run onboarding"
```

---

## Task 14: Full verification gate

- [ ] **Step 1: Rust — full workspace test + clippy**

Run:
```bash
cargo test --workspace 2>&1 | tail -20
cargo clippy --workspace -- -D warnings 2>&1 | tail -10
```
Expected: all tests PASS; clippy 0 warnings. Fix any failure at its root (do not delete assertions).

- [ ] **Step 2: Frontend — lint, typecheck, tests**

Run:
```bash
cd apps/web && bun run typecheck 2>&1 | tail -5 && bun run test 2>&1 | tail -10 && cd ../..
bun x ultracite check 2>&1 | tail -15
```
Expected: typecheck clean; all vitest pass; Ultracite reports no errors. If Ultracite flags the new files, run `bun x ultracite fix`, re-run `bun run typecheck` + `bun run test`, then re-check.

- [ ] **Step 3: Manual smoke (documented, not automated)**

Document in the PR description (do not automate here): build server, run with an empty data dir, confirm the warn banner prints a random password; log in via the SPA → forced to `/onboarding`; submit new password (+ optional rename) → land on dashboard; old password rejected, new password works; existing-data upgrade path: a pre-existing DB with users is unaffected (no banner, no forced change).

- [ ] **Step 4: Commit any lint fixes**

```bash
git add -A
git commit -m "chore: lint/format fixes for onboarding feature"
```

(Skip if nothing changed.)

- [ ] **Step 5: Finish the development branch**

Invoke the superpowers:finishing-a-development-branch skill to decide merge/PR. PR title/body in English (project convention); no AI attribution.

---

## Self-Review

**Spec coverage check (spec §→task):**
- §1 config layer (delete AdminConfig, always-random, doc removal) → Tasks 3, 13 ✓
- §2 data model (migration, entity field, init flag) → Tasks 1, 2, 3 ✓
- §3 hard-block (middleware custom 403 code, stripped-path whitelist, CurrentUser field, user-WS gating, agent WS excluded, MeResponse+LoginResponse fields, mobile login) → Tasks 4, 5, 7, 8 ✓
- §4 onboarding endpoint (OnboardingRequest, validation incl. trim/blank/dup/same-pw/flag, service, handler-owned audit, openapi) → Task 6 ✓
- §5 frontend (_authed guard + WS gating, onboarding page own-auth handling, default username = current user, api-client redirect via window.location.assign + ApiError.code) → Tasks 10, 11, 12 ✓
- §6 banner (warn-level, one-time wording, fixed admin username) → Task 3 Step 4 ✓
- §7 tests (init_admin, complete_onboarding matrix, integration harness reshape, blocked-route/whitelist/login+me fields/mobile/WS, vitest guard+mismatch) → Tasks 3, 6, 9, 12 ✓
- §"影响文件清单" all entries mapped in File Structure table ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; commands have expected output. ✓

**Type consistency:** `must_change_password: bool` (Rust) / `must_change_password` (TS via generated `MeResponse`/`LoginResponse`); `OnboardingRequest { new_password, new_username }` matches Rust struct field names exactly; `complete_onboarding(db, user_id, new_password, new_username: Option<&str>)` signature used identically in Task 6 handler and tests; `MUST_CHANGE_PASSWORD` code string identical in middleware, api-client, integration test, mobile message prefix. ✓
