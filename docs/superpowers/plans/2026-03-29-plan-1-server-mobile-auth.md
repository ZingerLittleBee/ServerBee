# Server Mobile Auth API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement server-side mobile authentication (opaque access token + refresh token rotation) with Bearer token middleware support, enabling iOS clients to authenticate via `/api/mobile/auth/*` endpoints.

**Architecture:** Three database migrations add `mobile_session` table, `source`/`mobile_session_id` columns to `session`, and `device_token` table. A new `mobile_auth` service handles token issuance/refresh/revocation. The existing `auth_middleware` and all WS auth functions gain a Bearer token path. `validate_session` becomes source-aware (no sliding renewal for mobile tokens; WS connections auto-close on mobile token expiry).

**Tech Stack:** Rust, Axum 0.8, sea-orm 1.x, SQLite, argon2, tokio

**Spec:** `docs/superpowers/specs/2026-03-29-ios-mvp-design.md` Sections 1 + 3

---

### Task 1: Migration — Create `mobile_session` table

**Files:**
- Create: `crates/server/src/migration/m20260329_000013_create_mobile_session.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create migration file**

```rust
// crates/server/src/migration/m20260329_000013_create_mobile_session.rs
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260329_000013_create_mobile_session"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS mobile_sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id),
                refresh_token_hash TEXT NOT NULL,
                installation_id TEXT NOT NULL,
                device_name TEXT NOT NULL DEFAULT '',
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at DATETIME NOT NULL,
                last_used_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_mobile_sessions_user_id ON mobile_sessions(user_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_mobile_sessions_installation_id ON mobile_sessions(installation_id)",
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

Add to `crates/server/src/migration/mod.rs`:

```rust
// After the last existing mod declaration (line 14):
mod m20260329_000013_create_mobile_session;

// Inside migrations() vec, after the last Box::new (line 32):
            Box::new(m20260329_000013_create_mobile_session::Migration),
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/m20260329_000013_create_mobile_session.rs crates/server/src/migration/mod.rs
git commit -m "feat(server): add mobile_session table migration"
```

---

### Task 2: Migration — Add `source` and `mobile_session_id` to `session` table

**Files:**
- Create: `crates/server/src/migration/m20260329_000014_add_session_source.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create migration file**

```rust
// crates/server/src/migration/m20260329_000014_add_session_source.rs
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260329_000014_add_session_source"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE sessions ADD COLUMN source TEXT NOT NULL DEFAULT 'web'",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE sessions ADD COLUMN mobile_session_id TEXT REFERENCES mobile_sessions(id)",
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

Add to `crates/server/src/migration/mod.rs`:

```rust
// After the previous mod declaration:
mod m20260329_000014_add_session_source;

// Inside migrations() vec:
            Box::new(m20260329_000014_add_session_source::Migration),
```

- [ ] **Step 3: Update session entity**

Modify `crates/server/src/entity/session.rs` — add two new fields to the Model struct:

```rust
// Add after `created_at` field (line 15):
    #[sea_orm(default_value = "web")]
    pub source: String,
    pub mobile_session_id: Option<String>,
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration/m20260329_000014_add_session_source.rs crates/server/src/migration/mod.rs crates/server/src/entity/session.rs
git commit -m "feat(server): add source and mobile_session_id to session table"
```

---

### Task 3: Migration — Create `device_token` table

**Files:**
- Create: `crates/server/src/migration/m20260329_000015_create_device_token.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create migration file**

```rust
// crates/server/src/migration/m20260329_000015_create_device_token.rs
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260329_000015_create_device_token"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS device_tokens (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id),
                mobile_session_id TEXT NOT NULL REFERENCES mobile_sessions(id),
                installation_id TEXT NOT NULL,
                token TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(installation_id)
            )",
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

Add to `crates/server/src/migration/mod.rs`:

```rust
mod m20260329_000015_create_device_token;

// Inside migrations() vec:
            Box::new(m20260329_000015_create_device_token::Migration),
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/m20260329_000015_create_device_token.rs crates/server/src/migration/mod.rs
git commit -m "feat(server): add device_token table migration"
```

---

### Task 4: Entity — `mobile_session` and `device_token`

**Files:**
- Create: `crates/server/src/entity/mobile_session.rs`
- Create: `crates/server/src/entity/device_token.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Create mobile_session entity**

```rust
// crates/server/src/entity/mobile_session.rs
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mobile_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    pub refresh_token_hash: String,
    pub installation_id: String,
    pub device_name: String,
    pub created_at: DateTimeUtc,
    pub expires_at: DateTimeUtc,
    pub last_used_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Create device_token entity**

```rust
// crates/server/src/entity/device_token.rs
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "device_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub user_id: String,
    pub mobile_session_id: String,
    pub installation_id: String,
    pub token: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::mobile_session::Entity",
        from = "Column::MobileSessionId",
        to = "super::mobile_session::Column::Id"
    )]
    MobileSession,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 3: Register entities in mod.rs**

Add to `crates/server/src/entity/mod.rs` (alphabetical order):

```rust
// After `pub mod dashboard_widget;` (line 7):
pub mod device_token;

// After `pub mod maintenance;` (line 12):
pub mod mobile_session;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/entity/mobile_session.rs crates/server/src/entity/device_token.rs crates/server/src/entity/mod.rs
git commit -m "feat(server): add mobile_session and device_token entities"
```

---

### Task 5: Config — Add `MobileConfig`

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Add MobileConfig struct**

Add after `FileConfig` struct and its `Default` impl in `crates/server/src/config.rs`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct MobileConfig {
    #[serde(default = "default_mobile_access_ttl")]
    pub access_ttl: i64,
    #[serde(default = "default_mobile_refresh_ttl")]
    pub refresh_ttl: i64,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            access_ttl: default_mobile_access_ttl(),
            refresh_ttl: default_mobile_refresh_ttl(),
        }
    }
}
```

- [ ] **Step 2: Add default functions**

Add to the defaults section (near other `fn default_*` functions):

```rust
fn default_mobile_access_ttl() -> i64 {
    900 // 15 minutes
}

fn default_mobile_refresh_ttl() -> i64 {
    2_592_000 // 30 days
}
```

- [ ] **Step 3: Add `mobile` field to `AppConfig`**

Add to the `AppConfig` struct:

```rust
    #[serde(default)]
    pub mobile: MobileConfig,
```

Add to `impl Default for AppConfig`:

```rust
            mobile: MobileConfig::default(),
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat(server): add MobileConfig for access/refresh TTL"
```

---

### Task 6: Service — `mobile_auth` (token issuance + refresh + revocation)

**Files:**
- Create: `crates/server/src/service/mobile_auth.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write the test skeleton**

Create `crates/server/src/service/mobile_auth.rs` with tests first:

```rust
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::MobileConfig;
use crate::entity::{device_token, mobile_session, session, user};
use crate::error::AppError;
use crate::service::auth::AuthService;

/// Parameters for mobile login.
pub struct MobileLoginParams<'a> {
    pub username: &'a str,
    pub password: &'a str,
    pub totp_code: Option<&'a str>,
    pub installation_id: &'a str,
    pub device_name: &'a str,
    pub ip: &'a str,
    pub user_agent: &'a str,
}

/// The token pair returned to the iOS client.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobileTokenResponse {
    pub access_token: String,
    pub access_expires_in_secs: i64,
    pub refresh_token: String,
    pub refresh_expires_in_secs: i64,
    pub token_type: String,
    pub user: MobileUserResponse,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobileUserResponse {
    pub id: String,
    pub username: String,
    pub role: String,
}

pub struct MobileAuthService;

impl MobileAuthService {
    /// Issue a new access + refresh token pair for a mobile device.
    /// Validates credentials (including 2FA), creates a `mobile_session` row
    /// and a `session` row with `source = "mobile"`.
    pub async fn login(
        db: &DatabaseConnection,
        params: MobileLoginParams<'_>,
        config: &MobileConfig,
    ) -> Result<MobileTokenResponse, AppError> {
        // Validate credentials (reuse AuthService logic)
        let user = user::Entity::find()
            .filter(user::Column::Username.eq(params.username))
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        if !AuthService::verify_password(params.password, &user.password_hash)? {
            return Err(AppError::Unauthorized);
        }

        // Check 2FA
        if let Some(ref secret) = user.totp_secret {
            match params.totp_code {
                Some(code) => {
                    if !AuthService::verify_totp(secret, code)? {
                        return Err(AppError::Unauthorized);
                    }
                }
                None => {
                    return Err(AppError::Validation("2fa_required".to_string()));
                }
            }
        }

        Self::issue_token_pair(db, &user, params.installation_id, params.device_name, params.ip, params.user_agent, config).await
    }

    /// Refresh: validate the refresh token, rotate to a new token pair.
    pub async fn refresh(
        db: &DatabaseConnection,
        refresh_token: &str,
        installation_id: &str,
        config: &MobileConfig,
    ) -> Result<MobileTokenResponse, AppError> {
        // Find all mobile_sessions for this installation_id
        let sessions = mobile_session::Entity::find()
            .filter(mobile_session::Column::InstallationId.eq(installation_id))
            .all(db)
            .await?;

        // Find the one whose hash matches
        let matched = sessions.into_iter().find(|ms| {
            Self::verify_refresh_token(refresh_token, &ms.refresh_token_hash).unwrap_or(false)
        });

        let ms = matched.ok_or(AppError::Unauthorized)?;

        // Check expiry
        if ms.expires_at < Utc::now() {
            // Clean up expired mobile session + its sessions
            Self::revoke_mobile_session(db, &ms.id).await?;
            return Err(AppError::Unauthorized);
        }

        // Get user
        let user = user::Entity::find_by_id(&ms.user_id)
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        // Revoke old tokens
        Self::revoke_mobile_session(db, &ms.id).await?;

        // Issue new pair
        Self::issue_token_pair(db, &user, &ms.installation_id, &ms.device_name, "", "", config).await
    }

    /// Logout: revoke the mobile session identified by the Bearer token's session row.
    pub async fn logout(
        db: &DatabaseConnection,
        mobile_session_id: &str,
    ) -> Result<(), AppError> {
        Self::revoke_mobile_session(db, mobile_session_id).await
    }

    /// List all active mobile devices for a user.
    pub async fn list_devices(
        db: &DatabaseConnection,
        user_id: &str,
    ) -> Result<Vec<mobile_session::Model>, AppError> {
        let devices = mobile_session::Entity::find()
            .filter(mobile_session::Column::UserId.eq(user_id))
            .filter(mobile_session::Column::ExpiresAt.gt(Utc::now()))
            .all(db)
            .await?;
        Ok(devices)
    }

    /// Revoke a specific mobile session (remote logout).
    pub async fn revoke_device(
        db: &DatabaseConnection,
        mobile_session_id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        // Verify ownership
        let ms = mobile_session::Entity::find_by_id(mobile_session_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Device not found".to_string()))?;

        if ms.user_id != user_id {
            return Err(AppError::NotFound("Device not found".to_string()));
        }

        Self::revoke_mobile_session(db, mobile_session_id).await
    }

    // ── Internal helpers ──

    /// Issue a fresh access token + refresh token pair.
    async fn issue_token_pair(
        db: &DatabaseConnection,
        user: &user::Model,
        installation_id: &str,
        device_name: &str,
        ip: &str,
        user_agent: &str,
        config: &MobileConfig,
    ) -> Result<MobileTokenResponse, AppError> {
        let now = Utc::now();
        let access_token = AuthService::generate_session_token();
        let refresh_token = Self::generate_refresh_token();
        let refresh_hash = Self::hash_refresh_token(&refresh_token)?;

        let mobile_session_id = Uuid::new_v4().to_string();

        // Create mobile_session row
        let ms = mobile_session::ActiveModel {
            id: Set(mobile_session_id.clone()),
            user_id: Set(user.id.clone()),
            refresh_token_hash: Set(refresh_hash),
            installation_id: Set(installation_id.to_string()),
            device_name: Set(device_name.to_string()),
            created_at: Set(now),
            expires_at: Set(now + chrono::Duration::seconds(config.refresh_ttl)),
            last_used_at: Set(now),
        };
        ms.insert(db).await?;

        // Create session row (access token) with source="mobile"
        let session = session::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user.id.clone()),
            token: Set(access_token.clone()),
            ip: Set(ip.to_string()),
            user_agent: Set(user_agent.to_string()),
            expires_at: Set(now + chrono::Duration::seconds(config.access_ttl)),
            created_at: Set(now),
            source: Set("mobile".to_string()),
            mobile_session_id: Set(Some(mobile_session_id)),
        };
        session.insert(db).await?;

        Ok(MobileTokenResponse {
            access_token,
            access_expires_in_secs: config.access_ttl,
            refresh_token,
            refresh_expires_in_secs: config.refresh_ttl,
            token_type: "Bearer".to_string(),
            user: MobileUserResponse {
                id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
            },
        })
    }

    /// Delete a mobile_session and all its associated session rows and device_token rows.
    async fn revoke_mobile_session(
        db: &DatabaseConnection,
        mobile_session_id: &str,
    ) -> Result<(), AppError> {
        // Delete device_tokens
        device_token::Entity::delete_many()
            .filter(device_token::Column::MobileSessionId.eq(mobile_session_id))
            .exec(db)
            .await?;

        // Delete access token sessions
        session::Entity::delete_many()
            .filter(session::Column::MobileSessionId.eq(mobile_session_id))
            .exec(db)
            .await?;

        // Delete mobile_session
        mobile_session::Entity::delete_by_id(mobile_session_id)
            .exec(db)
            .await?;

        Ok(())
    }

    fn generate_refresh_token() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn hash_refresh_token(token: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(token.as_bytes(), &salt)
            .map_err(|e| AppError::Internal(format!("Failed to hash refresh token: {e}")))?;
        Ok(hash.to_string())
    }

    fn verify_refresh_token(token: &str, hash: &str) -> Result<bool, AppError> {
        let parsed = PasswordHash::new(hash)
            .map_err(|e| AppError::Internal(format!("Invalid hash: {e}")))?;
        Ok(Argon2::default()
            .verify_password(token.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_token_hash_roundtrip() {
        let token = MobileAuthService::generate_refresh_token();
        let hash = MobileAuthService::hash_refresh_token(&token).unwrap();
        assert!(MobileAuthService::verify_refresh_token(&token, &hash).unwrap());
        assert!(!MobileAuthService::verify_refresh_token("wrong_token", &hash).unwrap());
    }
}
```

- [ ] **Step 2: Register service in mod.rs**

Add to `crates/server/src/service/mod.rs` (alphabetical):

```rust
// After `pub mod maintenance;` (line 13):
pub mod mobile_auth;
```

- [ ] **Step 3: Run the unit test**

Run: `cargo test -p serverbee-server test_refresh_token_hash_roundtrip -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/mobile_auth.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add mobile_auth service with token issuance and refresh"
```

---

### Task 7: Refactor `validate_session` — source-aware TTL + conditional sliding expiry

**Files:**
- Modify: `crates/server/src/service/auth.rs`
- Modify: `crates/server/src/middleware/auth.rs`
- Modify: `crates/server/src/router/ws/browser.rs`
- Modify: `crates/server/src/router/ws/terminal.rs`
- Modify: `crates/server/src/router/ws/docker_logs.rs`

- [ ] **Step 1: Change `validate_session` signature and logic**

In `crates/server/src/service/auth.rs`, replace the existing `validate_session` function (lines 136-171) with:

```rust
    /// Validate a session token. If valid and not expired, returns the associated user.
    /// For web sessions: extends expiration (sliding expiry).
    /// For mobile sessions: fixed expiry, no extension.
    /// Returns `(user, session)` so callers can inspect session metadata (source, mobile_session_id, expires_at).
    pub async fn validate_session(
        db: &DatabaseConnection,
        token: &str,
        web_session_ttl: i64,
    ) -> Result<Option<(user::Model, session::Model)>, AppError> {
        let session = session::Entity::find()
            .filter(session::Column::Token.eq(token))
            .one(db)
            .await?;

        let session = match session {
            Some(s) => s,
            None => return Ok(None),
        };

        // Check expiration
        if session.expires_at < Utc::now() {
            session::Entity::delete_by_id(&session.id)
                .exec(db)
                .await?;
            return Ok(None);
        }

        let user_id = session.user_id.clone();

        // Sliding expiry only for web sessions
        if session.source == "web" {
            let new_expires = Utc::now() + chrono::Duration::seconds(web_session_ttl);
            let mut active: session::ActiveModel = session.clone().into();
            active.expires_at = Set(new_expires);
            active.update(db).await?;
        }

        let user = user::Entity::find_by_id(&user_id).one(db).await?;

        match user {
            Some(u) => Ok(Some((u, session))),
            None => Ok(None),
        }
    }
```

- [ ] **Step 2: Update middleware/auth.rs**

The `auth_middleware` currently calls `validate_session` and expects `Option<user::Model>`. Update to match the new return type and add Bearer token path.

Replace the `auth_middleware` function body in `crates/server/src/middleware/auth.rs`:

```rust
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    // Try session cookie
    let current_user = if let Some(token) = extract_session_cookie(&req) {
        AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl)
            .await
            .ok()
            .flatten()
            .map(|(user, _session)| CurrentUser {
                user_id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
            })
    } else {
        None
    };

    // Try API key header if session not found
    let current_user = match current_user {
        Some(u) => Some(u),
        None => {
            if let Some(key) = extract_api_key(&req) {
                AuthService::validate_api_key(&state.db, &key)
                    .await
                    .ok()
                    .flatten()
                    .map(|user| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                    })
            } else {
                None
            }
        }
    };

    // Try Bearer token
    let current_user = match current_user {
        Some(u) => Some(u),
        None => {
            if let Some(token) = extract_bearer_token(&req) {
                AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl)
                    .await
                    .ok()
                    .flatten()
                    .map(|(user, _session)| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                    })
            } else {
                None
            }
        }
    };

    match current_user {
        Some(user) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}
```

Add the `extract_bearer_token` helper function:

```rust
fn extract_bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}
```

- [ ] **Step 3: Update browser.rs WS auth**

In `crates/server/src/router/ws/browser.rs`, update `validate_browser_auth`:

```rust
async fn validate_browser_auth(state: &Arc<AppState>, headers: &HeaderMap) -> Option<(String, Option<chrono::DateTime<chrono::Utc>>, String)> {
    // Try session cookie
    if let Some(token) = extract_session_cookie(headers)
        && let Ok(Some((user, session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        return Some((user.id, None, session.source));
    }

    // Try API key header
    if let Some(key) = extract_api_key(headers)
        && let Ok(Some(user)) = AuthService::validate_api_key(&state.db, &key).await
    {
        return Some((user.id, None, "web".to_string()));
    }

    // Try Bearer token
    if let Some(token) = extract_bearer_token(headers)
        && let Ok(Some((user, session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        let expires = if session.source == "mobile" { Some(session.expires_at) } else { None };
        return Some((user.id, expires, session.source));
    }

    None
}
```

Add `extract_bearer_token` helper (same as in auth.rs):

```rust
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}
```

Update `browser_ws_handler` to pass expiry into the WS handler and close on token expiry:

```rust
async fn browser_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let auth = validate_browser_auth(&state, &headers).await;
    match auth {
        Some((_, mobile_expires, _)) => ws
            .max_message_size(MAX_WS_MESSAGE_SIZE)
            .on_upgrade(move |socket| handle_browser_ws(socket, state, mobile_expires)),
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}
```

Update `handle_browser_ws` signature to accept the optional expiry, and add a `tokio::time::sleep_until` branch in the select loop:

In the `loop { tokio::select! { ... } }`, add a new branch:

```rust
// Add inside the tokio::select! block, before other branches:
            _ = async {
                if let Some(exp) = mobile_expires {
                    let dur = (exp - chrono::Utc::now()).to_std().unwrap_or_default();
                    tokio::time::sleep(dur).await;
                } else {
                    // Web sessions: never expire the WS connection
                    std::future::pending::<()>().await;
                }
            } => {
                tracing::debug!("Mobile WS token expired, closing connection");
                let _ = ws_sink.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 4001,
                    reason: "token expired".into(),
                }))).await;
                break;
            }
```

- [ ] **Step 4: Update terminal.rs and docker_logs.rs WS auth**

Apply the same Bearer token addition pattern to `validate_auth` in both `crates/server/src/router/ws/terminal.rs` and `crates/server/src/router/ws/docker_logs.rs`. These are simpler — they don't need the expiry tracking (terminal and docker are not available to mobile MVP), but the Bearer path must be present for consistency:

In both files, add after the API key check in `validate_auth`:

```rust
    // Try Bearer token
    if let Some(token) = extract_bearer_token(headers)
        && let Ok(Some((user, _session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        return Some(user.id); // or Some((user.id, user.role)) for terminal.rs
    }
```

Add `extract_bearer_token` helper to each file.

- [ ] **Step 5: Fix auth.rs tests**

Update the two existing tests in `crates/server/src/service/auth.rs` to match the new return type of `validate_session`:

```rust
    #[tokio::test]
    async fn test_validate_session_valid() {
        let db = setup_test_db().await;
        let session = create_test_session(&db).await;
        let validated = AuthService::validate_session(&db, &session.token, 3600)
            .await
            .expect("validate_session should not error");
        assert!(validated.is_some());
        let (user, sess) = validated.unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(sess.source, "web");
    }

    #[tokio::test]
    async fn test_validate_session_invalid_token() {
        let db = setup_test_db().await;
        let result = AuthService::validate_session(&db, "fake_token_that_does_not_exist", 3600)
            .await
            .expect("validate_session should not error");
        assert!(result.is_none());
    }
```

- [ ] **Step 6: Run all tests**

Run: `cargo test -p serverbee-server 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/service/auth.rs crates/server/src/middleware/auth.rs crates/server/src/router/ws/browser.rs crates/server/src/router/ws/terminal.rs crates/server/src/router/ws/docker_logs.rs
git commit -m "feat(server): source-aware validate_session + Bearer token middleware"
```

---

### Task 8: AppState — Add `pending_pairs` DashMap

**Files:**
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Add PendingPair struct and DashMap**

In `crates/server/src/state.rs`, add after the `PendingTotp` struct:

```rust
/// Pending mobile pairing code, keyed by code string.
pub struct PendingPair {
    pub user_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

Add the field to `AppState`:

```rust
    /// Pending mobile pairing codes for QR login, keyed by code.
    pub pending_pairs: DashMap<String, PendingPair>,
```

Add initialization in `AppState::new()`:

```rust
            pending_pairs: DashMap::new(),
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/state.rs
git commit -m "feat(server): add pending_pairs DashMap to AppState"
```

---

### Task 9: Router — Mobile auth endpoints

**Files:**
- Create: `crates/server/src/router/api/mobile.rs`
- Modify: `crates/server/src/router/api/mod.rs`

- [ ] **Step 1: Create the mobile router**

```rust
// crates/server/src/router/api/mobile.rs
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ok, ApiResponse, AppError};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::mobile_auth::{MobileAuthService, MobileLoginParams, MobileTokenResponse};
use crate::state::AppState;

// ── Request/Response DTOs ──

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MobileLoginRequest {
    username: String,
    password: String,
    installation_id: String,
    #[serde(default)]
    device_name: String,
    totp_code: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MobileRefreshRequest {
    refresh_token: String,
    installation_id: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MobilePairRedeemRequest {
    code: String,
    installation_id: String,
    #[serde(default)]
    device_name: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobilePairCodeResponse {
    code: String,
    expires_in_secs: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobileDeviceResponse {
    id: String,
    device_name: String,
    installation_id: String,
    created_at: String,
    last_used_at: String,
}

// ── Routes ──

/// Public routes (no auth required).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/auth/login", post(mobile_login))
        .route("/mobile/auth/refresh", post(mobile_refresh))
        .route("/mobile/auth/pair", post(mobile_pair_redeem))
}

/// Protected routes (auth required).
pub fn protected_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/auth/logout", post(mobile_logout))
        .route("/mobile/auth/devices", get(list_devices))
        .route("/mobile/auth/devices/{id}", delete(revoke_device))
        .route("/mobile/pair", post(generate_pair_code))
}

// ── Handlers ──

#[utoipa::path(
    post,
    path = "/api/mobile/auth/login",
    tag = "mobile-auth",
    request_body = MobileLoginRequest,
    responses(
        (status = 200, description = "Login successful", body = MobileTokenResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 422, description = "2FA code required"),
        (status = 429, description = "Too many attempts"),
    )
)]
async fn mobile_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    Json(body): Json<MobileLoginRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    if !state.check_login_rate(&ip) {
        return Err(AppError::TooManyRequests(
            "Too many login attempts. Please try again later.".to_string(),
        ));
    }

    let user_agent = req_headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let response = MobileAuthService::login(
        &state.db,
        MobileLoginParams {
            username: &body.username,
            password: &body.password,
            totp_code: body.totp_code.as_deref(),
            installation_id: &body.installation_id,
            device_name: &body.device_name,
            ip: &ip,
            user_agent: &user_agent,
        },
        &state.config.mobile,
    )
    .await?;

    ok(response)
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/refresh",
    tag = "mobile-auth",
    request_body = MobileRefreshRequest,
    responses(
        (status = 200, description = "Token refreshed", body = MobileTokenResponse),
        (status = 401, description = "Invalid or expired refresh token"),
    )
)]
async fn mobile_refresh(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MobileRefreshRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    let response = MobileAuthService::refresh(
        &state.db,
        &body.refresh_token,
        &body.installation_id,
        &state.config.mobile,
    )
    .await?;
    ok(response)
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/logout",
    tag = "mobile-auth",
    responses(
        (status = 200, description = "Logged out"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_token" = []))
)]
async fn mobile_logout(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    req_headers: HeaderMap,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    // Find the session by Bearer token to get mobile_session_id
    let token = req_headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let session = crate::entity::session::Entity::find()
        .filter(crate::entity::session::Column::Token.eq(token))
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let mobile_session_id = session
        .mobile_session_id
        .ok_or(AppError::BadRequest("Not a mobile session".to_string()))?;

    MobileAuthService::logout(&state.db, &mobile_session_id).await?;
    ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/mobile/auth/devices",
    tag = "mobile-auth",
    responses(
        (status = 200, description = "List of devices", body = Vec<MobileDeviceResponse>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_devices(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<Vec<MobileDeviceResponse>>>, AppError> {
    let devices = MobileAuthService::list_devices(&state.db, &current_user.user_id).await?;
    let response: Vec<MobileDeviceResponse> = devices
        .into_iter()
        .map(|d| MobileDeviceResponse {
            id: d.id,
            device_name: d.device_name,
            installation_id: d.installation_id,
            created_at: d.created_at.to_rfc3339(),
            last_used_at: d.last_used_at.to_rfc3339(),
        })
        .collect();
    ok(response)
}

#[utoipa::path(
    delete,
    path = "/api/mobile/auth/devices/{id}",
    tag = "mobile-auth",
    params(("id" = String, Path, description = "Mobile session ID")),
    responses(
        (status = 200, description = "Device revoked"),
        (status = 404, description = "Device not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn revoke_device(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    MobileAuthService::revoke_device(&state.db, &id, &current_user.user_id).await?;
    ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/mobile/pair",
    tag = "mobile-auth",
    responses(
        (status = 200, description = "Pairing code generated", body = MobilePairCodeResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn generate_pair_code(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<MobilePairCodeResponse>>, AppError> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use rand::RngCore;

    // Remove any existing code for this user
    state
        .pending_pairs
        .retain(|_, v| v.user_id != current_user.user_id);

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let code = format!("sb_pair_{}", URL_SAFE_NO_PAD.encode(bytes));

    state.pending_pairs.insert(
        code.clone(),
        crate::state::PendingPair {
            user_id: current_user.user_id,
            created_at: chrono::Utc::now(),
        },
    );

    ok(MobilePairCodeResponse {
        code,
        expires_in_secs: 300,
    })
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/pair",
    tag = "mobile-auth",
    request_body = MobilePairRedeemRequest,
    responses(
        (status = 200, description = "Pairing successful", body = MobileTokenResponse),
        (status = 401, description = "Invalid or expired code"),
    )
)]
async fn mobile_pair_redeem(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    Json(body): Json<MobilePairRedeemRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    // Look up and remove the pairing code
    let pair = state
        .pending_pairs
        .remove(&body.code)
        .ok_or(AppError::Unauthorized)?;
    let (_, pair) = pair;

    // Check TTL (5 minutes)
    if chrono::Utc::now() - pair.created_at > chrono::Duration::minutes(5) {
        return Err(AppError::Unauthorized);
    }

    // Get user
    let user = crate::entity::user::Entity::find_by_id(&pair.user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let user_agent = req_headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Issue tokens using the internal helper
    let response = MobileAuthService::login_for_user(
        &state.db,
        &user,
        &body.installation_id,
        &body.device_name,
        &ip,
        &user_agent,
        &state.config.mobile,
    )
    .await?;

    ok(response)
}
```

- [ ] **Step 2: Add `login_for_user` to MobileAuthService**

Add this public method to `MobileAuthService` in `crates/server/src/service/mobile_auth.rs`:

```rust
    /// Issue token pair for an already-authenticated user (used by QR pairing).
    pub async fn login_for_user(
        db: &DatabaseConnection,
        user: &user::Model,
        installation_id: &str,
        device_name: &str,
        ip: &str,
        user_agent: &str,
        config: &MobileConfig,
    ) -> Result<MobileTokenResponse, AppError> {
        Self::issue_token_pair(db, user, installation_id, device_name, ip, user_agent, config).await
    }
```

- [ ] **Step 3: Mount mobile router in api/mod.rs**

In `crates/server/src/router/api/mod.rs`, add `pub mod mobile;` at the top, then mount:

```rust
// In the public section (after line 39: .merge(auth::public_router())):
        .merge(mobile::public_router())

// In the authenticated section (after auth::protected_router(), line 45):
                .merge(mobile::protected_router())
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -10`
Expected: Compiles without errors.

- [ ] **Step 5: Run all tests**

Run: `cargo test -p serverbee-server 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/api/mobile.rs crates/server/src/router/api/mod.rs crates/server/src/service/mobile_auth.rs
git commit -m "feat(server): add mobile auth endpoints (login, refresh, logout, pair, devices)"
```

---

### Task 10: OpenAPI — Register new endpoints and Bearer security scheme

**Files:**
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Add new paths and security scheme**

In `crates/server/src/openapi.rs`, add the new endpoints to the `paths(...)` list:

```rust
        // mobile auth
        crate::router::api::mobile::mobile_login,
        crate::router::api::mobile::mobile_refresh,
        crate::router::api::mobile::mobile_logout,
        crate::router::api::mobile::list_devices,
        crate::router::api::mobile::revoke_device,
        crate::router::api::mobile::generate_pair_code,
        crate::router::api::mobile::mobile_pair_redeem,
```

Add the new DTOs to `components(schemas(...))`:

```rust
        crate::router::api::mobile::MobileLoginRequest,
        crate::router::api::mobile::MobileRefreshRequest,
        crate::router::api::mobile::MobilePairRedeemRequest,
        crate::router::api::mobile::MobilePairCodeResponse,
        crate::router::api::mobile::MobileDeviceResponse,
        crate::service::mobile_auth::MobileTokenResponse,
        crate::service::mobile_auth::MobileUserResponse,
```

Add `bearer_token` to the `security_schemes` in the `modifiers(...)` section. If there's an existing `SecurityAddon` modifier, add:

```rust
        ("bearer_token", SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("opaque")
                .build(),
        )),
```

- [ ] **Step 2: Add `("bearer_token" = [])` to existing security annotations**

Batch-replace across all `router/api/*.rs` files: change `security(("session_cookie" = []), ("api_key" = []))` to `security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/openapi.rs crates/server/src/router/api/
git commit -m "feat(server): register mobile endpoints in OpenAPI + bearer security scheme"
```

---

### Task 11: Documentation — ENV.md and config docs

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`

- [ ] **Step 1: Update ENV.md**

Add to the appropriate section in `ENV.md`:

```markdown
### Mobile

| Variable | Default | Description |
|---|---|---|
| `SERVERBEE_MOBILE__ACCESS_TTL` | `900` | Mobile access token lifetime in seconds (15 min) |
| `SERVERBEE_MOBILE__REFRESH_TTL` | `2592000` | Mobile refresh token lifetime in seconds (30 days) |
```

- [ ] **Step 2: Update English config docs**

Add `[mobile]` section to `apps/docs/content/docs/en/configuration.mdx`:

```markdown
### Mobile

Configuration for the mobile (iOS) app authentication.

| Key | Type | Default | Description |
|---|---|---|---|
| `mobile.access_ttl` | integer | `900` | Access token lifetime in seconds (15 min) |
| `mobile.refresh_ttl` | integer | `2592000` | Refresh token lifetime in seconds (30 days) |
```

- [ ] **Step 3: Update Chinese config docs**

Add `[mobile]` section to `apps/docs/content/docs/cn/configuration.mdx`:

```markdown
### 移动端

移动端（iOS）应用认证配置。

| 键 | 类型 | 默认值 | 描述 |
|---|---|---|---|
| `mobile.access_ttl` | 整数 | `900` | 访问令牌有效期（秒），默认 15 分钟 |
| `mobile.refresh_ttl` | 整数 | `2592000` | 刷新令牌有效期（秒），默认 30 天 |
```

- [ ] **Step 4: Commit**

```bash
git add ENV.md apps/docs/content/docs/en/configuration.mdx apps/docs/content/docs/cn/configuration.mdx
git commit -m "docs: add mobile config env vars and configuration docs"
```

---

### Task 12: Alert detail endpoint

**Files:**
- Modify: `crates/server/src/router/api/alert.rs`

- [ ] **Step 1: Add the alert event detail endpoint**

In `crates/server/src/router/api/alert.rs`, add to `alert_events_router()`:

```rust
pub fn alert_events_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/alert-events", get(list_alert_events))
        .route("/alert-events/{alert_key}", get(get_alert_event_detail))
}
```

Add the response DTO:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AlertEventDetailResponse {
    pub alert_key: String,
    pub rule_id: String,
    pub rule_name: String,
    pub server_id: String,
    pub server_name: String,
    pub status: String,
    pub message: String,
    pub trigger_count: i32,
    pub first_triggered_at: String,
    pub resolved_at: Option<String>,
    pub rule_enabled: bool,
    pub rule_trigger_mode: String,
}
```

Add the handler:

```rust
#[utoipa::path(
    get,
    path = "/api/alert-events/{alert_key}",
    tag = "alert-rules",
    params(("alert_key" = String, Path, description = "Alert key (rule_id:server_id)")),
    responses(
        (status = 200, description = "Alert event detail", body = AlertEventDetailResponse),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_alert_event_detail(
    State(state): State<Arc<AppState>>,
    Path(alert_key): Path<String>,
) -> Result<Json<ApiResponse<AlertEventDetailResponse>>, AppError> {
    // alert_key format: "rule_id:server_id"
    let parts: Vec<&str> = alert_key.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(AppError::NotFound("Invalid alert key format".to_string()));
    }
    let (rule_id, server_id) = (parts[0], parts[1]);

    let state_row = alert_state::Entity::find()
        .filter(alert_state::Column::RuleId.eq(rule_id))
        .filter(alert_state::Column::ServerId.eq(server_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Alert event not found".to_string()))?;

    let rule = alert_rule::Entity::find_by_id(rule_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Alert rule not found".to_string()))?;

    let server = server::Entity::find_by_id(server_id)
        .one(&state.db)
        .await?;
    let server_name = server.map(|s| s.name).unwrap_or_default();

    let status = if state_row.resolved { "resolved" } else { "firing" };

    ok(AlertEventDetailResponse {
        alert_key,
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        server_id: server_id.to_string(),
        server_name,
        status: status.to_string(),
        message: format!("{} alert on server", rule.name),
        trigger_count: state_row.count,
        first_triggered_at: state_row.first_triggered_at.to_rfc3339(),
        resolved_at: state_row.resolved_at.map(|t| t.to_rfc3339()),
        rule_enabled: rule.enabled,
        rule_trigger_mode: rule.trigger_mode.clone(),
    })
}
```

- [ ] **Step 2: Register in OpenAPI**

Add to `crates/server/src/openapi.rs` paths:

```rust
        crate::router::api::alert::get_alert_event_detail,
```

Add to schemas:

```rust
        crate::router::api::alert::AlertEventDetailResponse,
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 4: Run all tests**

Run: `cargo test -p serverbee-server 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/alert.rs crates/server/src/openapi.rs
git commit -m "feat(server): add /api/alert-events/{alert_key} detail endpoint"
```

---

### Task 13: Final verification

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 2: Run all Rust tests**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -10`
Expected: No warnings.

- [ ] **Step 4: Verify server starts and migrations run**

Run: `cargo run -p serverbee-server &` (let it start, check for migration logs, then kill)
Expected: Logs should show all migrations running without errors, including the three new ones.

- [ ] **Step 5: Verify Swagger UI shows new endpoints**

Open `http://localhost:9527/swagger-ui/` and check:
- `/api/mobile/auth/login` (POST)
- `/api/mobile/auth/refresh` (POST)
- `/api/mobile/auth/logout` (POST)
- `/api/mobile/auth/devices` (GET)
- `/api/mobile/auth/devices/{id}` (DELETE)
- `/api/mobile/pair` (POST)
- `/api/mobile/auth/pair` (POST)
- `/api/alert-events/{alert_key}` (GET)
- Bearer token security scheme visible
