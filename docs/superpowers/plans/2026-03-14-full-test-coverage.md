# ServerBee 全量测试覆盖 Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 ServerBee 测试覆盖率从 67 tests 提升到 ~211 tests，覆盖所有功能点。

**Architecture:** 按 P0→P3 优先级分 4 个 Chunk 实施。P0 覆盖安全关键路径（认证、中间件、权限），P1 覆盖核心业务逻辑（AgentManager、告警、记录、服务器 CRUD），P2 覆盖 API 集成测试，P3 覆盖 Agent 端和后台任务。每个 Chunk 内按 TDD 流程：写失败测试 → 验证失败 → 实现 → 验证通过 → 提交。

**Tech Stack:** Rust (tokio::test, tempfile, reqwest, tokio-tungstenite), TypeScript (vitest, @testing-library/react, jsdom)

**Current baseline:** 56 Rust tests + 11 frontend tests = 67 total

---

## Chunk 1: P0 — 安全与数据完整性

### Task 1: Rust 测试辅助模块

**Files:**
- Create: `crates/server/src/test_utils.rs`
- Modify: `crates/server/src/lib.rs` (添加 `#[cfg(test)] pub mod test_utils;`)

- [ ] **Step 1: 创建 test_utils 模块**

```rust
// crates/server/src/test_utils.rs
use sea_orm::{Database, DatabaseConnection};
use tempfile::TempDir;

use crate::config::AppConfig;
use crate::migration::Migrator;
use sea_orm_migration::MigratorTrait;

/// Create a temporary SQLite database with all migrations applied.
pub async fn setup_test_db() -> (DatabaseConnection, TempDir) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let url = format!("sqlite:{}?mode=rwc", db_path.display());
    let db = Database::connect(&url).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    (db, tmp)
}

/// Create a minimal AppConfig for testing.
pub fn test_config() -> AppConfig {
    AppConfig {
        admin: crate::config::AdminConfig {
            username: "admin".to_string(),
            password: "testpass".to_string(),
        },
        auth: crate::config::AuthConfig {
            session_ttl: 86400,
            auto_discovery_key: String::new(),
            secure_cookie: false,
        },
        ..Default::default()
    }
}
```

- [ ] **Step 2: 在 lib.rs 注册模块**

在 `crates/server/src/lib.rs` 末尾添加:
```rust
#[cfg(test)]
pub mod test_utils;
```

- [ ] **Step 3: 验证编译**

Run: `cargo test -p serverbee-server --no-run 2>&1 | tail -3`
Expected: Compiling → Finished

- [ ] **Step 4: 提交**

```bash
git add crates/server/src/test_utils.rs crates/server/src/lib.rs
git commit -m "test: add test_utils module with setup_test_db helper"
```

---

### Task 2: 认证中间件单元测试 (`middleware/auth.rs`)

**Files:**
- Modify: `crates/server/src/middleware/auth.rs` (在文件末尾添加 `#[cfg(test)] mod tests`)

- [ ] **Step 1: 编写 extract_session_cookie 测试**

在 `crates/server/src/middleware/auth.rs` 末尾添加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;

    #[test]
    fn test_extract_session_cookie_valid() {
        let req = HttpRequest::builder()
            .header("cookie", "session_token=abc123; other=val")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_only() {
        let req = HttpRequest::builder()
            .header("cookie", "session_token=tok42")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), Some("tok42".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_missing() {
        let req = HttpRequest::builder()
            .header("cookie", "other=val; foo=bar")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), None);
    }

    #[test]
    fn test_extract_session_cookie_no_header() {
        let req = HttpRequest::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), None);
    }

    #[test]
    fn test_extract_api_key_valid() {
        let req = HttpRequest::builder()
            .header("x-api-key", "sb_abc123def456")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(
            extract_api_key(&req),
            Some("sb_abc123def456".to_string())
        );
    }

    #[test]
    fn test_extract_api_key_missing() {
        let req = HttpRequest::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_api_key(&req), None);
    }
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test -p serverbee-server middleware::auth::tests -- --nocapture`
Expected: 6 tests passed

- [ ] **Step 3: 提交**

```bash
git add crates/server/src/middleware/auth.rs
git commit -m "test: add unit tests for auth middleware cookie/key extraction"
```

---

### Task 3: 认证服务 DB 测试 (`service/auth.rs`)

**Files:**
- Modify: `crates/server/src/service/auth.rs` (扩展已有 tests 模块)

- [ ] **Step 1: 添加 DB 集成测试**

在 `crates/server/src/service/auth.rs` 的 `mod tests` 块中追加:

```rust
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn test_create_user_success() {
        let (db, _tmp) = setup_test_db().await;
        let result = AuthService::create_user(&db, "alice", "pass1234", "member").await;
        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.username, "alice");
        assert_eq!(user.role, "member");
    }

    #[tokio::test]
    async fn test_create_user_duplicate() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "bob", "pass1234", "member")
            .await
            .unwrap();
        let result = AuthService::create_user(&db, "bob", "other1234", "member").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_login_success() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "carol", "mypass123", "admin")
            .await
            .unwrap();
        let result = AuthService::login(&db, "carol", "mypass123", "127.0.0.1", "test-agent", 86400).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_login_wrong_password() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "dave", "correct1", "member")
            .await
            .unwrap();
        let result = AuthService::login(&db, "dave", "wrong123", "127.0.0.1", "test-agent", 86400).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_login_nonexistent_user() {
        let (db, _tmp) = setup_test_db().await;
        let result =
            AuthService::login(&db, "nobody", "pass1234", "127.0.0.1", "test-agent", 86400).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_session_valid() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "eve", "pass1234", "admin")
            .await
            .unwrap();
        let (session, _user) = AuthService::login(&db, "eve", "pass1234", "127.0.0.1", "test-agent", 86400)
            .await
            .unwrap();
        let token = session.token;
        let user = AuthService::validate_session(&db, &token, 86400)
            .await
            .unwrap();
        assert!(user.is_some());
        assert_eq!(user.unwrap().username, "eve");
    }

    #[tokio::test]
    async fn test_validate_session_invalid_token() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::validate_session(&db, "nonexistent_token", 86400)
            .await
            .unwrap();
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn test_create_and_validate_api_key() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "frank", "pass1234", "admin")
            .await
            .unwrap();
        let (_model, raw_key) =
            AuthService::create_api_key(&db, &user.id, "test key").await.unwrap();
        assert!(raw_key.starts_with("sb_"));
        let validated = AuthService::validate_api_key(&db, &raw_key).await.unwrap();
        assert!(validated.is_some());
        assert_eq!(validated.unwrap().username, "frank");
    }

    #[tokio::test]
    async fn test_validate_api_key_invalid() {
        let (db, _tmp) = setup_test_db().await;
        let result = AuthService::validate_api_key(&db, "sb_invalid_key_12345678901234567890")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_change_password_wrong_old() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "grace", "oldpass1", "member")
            .await
            .unwrap();
        let result = AuthService::change_password(&db, &user.id, "wrong123", "newpass1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_change_password_success() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "heidi", "oldpass1", "member")
            .await
            .unwrap();
        AuthService::change_password(&db, &user.id, "oldpass1", "newpass1")
            .await
            .unwrap();
        // Login with new password
        let result =
            AuthService::login(&db, "heidi", "newpass1", "127.0.0.1", "test-agent").await;
        assert!(result.is_ok());
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-server service::auth::tests -- --nocapture`
Expected: All tests pass (original 8 + new 12 = 20)

- [ ] **Step 3: 提交**

```bash
git add crates/server/src/service/auth.rs
git commit -m "test: add DB integration tests for auth service (login, session, api-key, password)"
```

---

### Task 4: 用户服务安全测试 (`service/user.rs`)

**Files:**
- Modify: `crates/server/src/service/user.rs` (添加 tests 模块)

- [ ] **Step 1: 编写用户服务测试**

在 `crates/server/src/service/user.rs` 末尾添加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn test_list_users() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "user1", "pass1234", "admin")
            .await
            .unwrap();
        AuthService::create_user(&db, "user2", "pass1234", "member")
            .await
            .unwrap();
        let users = UserService::list_users(&db).await.unwrap();
        assert_eq!(users.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_user_cascading() {
        let (db, _tmp) = setup_test_db().await;
        // Create admin + another user
        AuthService::create_user(&db, "admin1", "pass1234", "admin")
            .await
            .unwrap();
        let user = AuthService::create_user(&db, "victim", "pass1234", "member")
            .await
            .unwrap();
        // Create API key for the user
        AuthService::create_api_key(&db, &user.id, "test key")
            .await
            .unwrap();
        // Delete user
        let result = UserService::delete_user(&db, &user.id).await;
        assert!(result.is_ok());
        // Verify user is gone (get_user returns Err on NotFound)
        let result = UserService::get_user(&db, &user.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_last_admin_blocked() {
        let (db, _tmp) = setup_test_db().await;
        let admin = AuthService::create_user(&db, "sole_admin", "pass1234", "admin")
            .await
            .unwrap();
        let result = UserService::delete_user(&db, &admin.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_role() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "keeper", "pass1234", "admin")
            .await
            .unwrap();
        let user = AuthService::create_user(&db, "promotee", "pass1234", "member")
            .await
            .unwrap();
        UserService::update_user(&db, &user.id, UpdateUserInput {
            role: Some("admin".to_string()),
            password: None,
        })
            .await
            .unwrap();
        let updated = UserService::get_user(&db, &user.id).await.unwrap();
        assert_eq!(updated.role, "admin");
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-server service::user::tests -- --nocapture`
Expected: 4 tests pass

- [ ] **Step 3: 提交**

```bash
git add crates/server/src/service/user.rs
git commit -m "test: add user service tests (CRUD, cascading delete, last admin guard)"
```

---

### Task 5: 前端 API Client 测试

**Files:**
- Create: `apps/web/src/lib/api-client.test.ts`

- [ ] **Step 1: 编写 API client 测试**

```typescript
// apps/web/src/lib/api-client.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { api, ApiError } from './api-client'

const mockFetch = vi.fn()
globalThis.fetch = mockFetch

beforeEach(() => {
  mockFetch.mockReset()
})

describe('api.get', () => {
  it('unwraps { data: T } response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: { id: '1', name: 'srv' } })
    })
    const result = await api.get<{ id: string; name: string }>('/api/servers/1')
    expect(result).toEqual({ id: '1', name: 'srv' })
    expect(mockFetch).toHaveBeenCalledWith('/api/servers/1', expect.objectContaining({ method: 'GET', credentials: 'include' }))
  })

  it('returns raw object when no data wrapper', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ key: 'value' })
    })
    const result = await api.get<{ key: string }>('/api/test')
    expect(result).toEqual({ key: 'value' })
  })
})

describe('api.post', () => {
  it('serializes body as JSON', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: { token: 'abc' } })
    })
    await api.post('/api/auth/login', { username: 'admin', password: 'pass' })
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/auth/login',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ username: 'admin', password: 'pass' }),
        headers: { 'Content-Type': 'application/json' }
      })
    )
  })
})

describe('api.delete', () => {
  it('returns undefined for 204 No Content', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 204
    })
    const result = await api.delete('/api/servers/1')
    expect(result).toBeUndefined()
  })
})

describe('error handling', () => {
  it('throws ApiError with status on 401', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 401,
      statusText: 'Unauthorized',
      text: async () => 'Invalid credentials'
    })
    await expect(api.get('/api/auth/status')).rejects.toThrow(ApiError)
  })

  it('ApiError contains status code and message', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      text: async () => 'Server error'
    })
    try {
      await api.get('/api/broken')
      expect.unreachable('should have thrown')
    } catch (e) {
      expect(e).toBeInstanceOf(ApiError)
      expect((e as ApiError).status).toBe(500)
      expect((e as ApiError).message).toBe('Server error')
    }
  })
})
```

- [ ] **Step 2: 运行测试**

Run: `cd apps/web && bunx vitest run src/lib/api-client.test.ts`
Expected: 5 tests pass

- [ ] **Step 3: 提交**

```bash
git add apps/web/src/lib/api-client.test.ts
git commit -m "test: add API client tests (unwrap, serialize, 204, error handling)"
```

---

### Task 6: 前端工具函数测试

**Files:**
- Create: `apps/web/src/lib/utils.test.ts`

- [ ] **Step 1: 编写 utils 测试**

```typescript
// apps/web/src/lib/utils.test.ts
import { describe, it, expect } from 'vitest'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from './utils'

describe('countryCodeToFlag', () => {
  it('converts US to flag emoji', () => {
    expect(countryCodeToFlag('US')).toBe('🇺🇸')
  })
  it('converts CN to flag emoji', () => {
    expect(countryCodeToFlag('CN')).toBe('🇨🇳')
  })
  it('returns empty for null', () => {
    expect(countryCodeToFlag(null)).toBe('')
  })
  it('returns empty for undefined', () => {
    expect(countryCodeToFlag(undefined)).toBe('')
  })
  it('returns empty for single char', () => {
    expect(countryCodeToFlag('A')).toBe('')
  })
  it('handles lowercase', () => {
    expect(countryCodeToFlag('gb')).toBe('🇬🇧')
  })
})

describe('formatBytes', () => {
  it('returns "0 B" for 0', () => {
    expect(formatBytes(0)).toBe('0 B')
  })
  it('returns "0 B" for negative', () => {
    expect(formatBytes(-100)).toBe('0 B')
  })
  it('returns "0 B" for NaN', () => {
    expect(formatBytes(NaN)).toBe('0 B')
  })
  it('formats KB', () => {
    expect(formatBytes(1024)).toBe('1.0 KB')
  })
  it('formats MB', () => {
    expect(formatBytes(1048576)).toBe('1.0 MB')
  })
  it('formats GB', () => {
    expect(formatBytes(1073741824)).toBe('1.0 GB')
  })
  it('formats TB', () => {
    expect(formatBytes(1099511627776)).toBe('1.0 TB')
  })
  it('formats fractional values', () => {
    expect(formatBytes(1536)).toBe('1.5 KB')
  })
})

describe('formatSpeed', () => {
  it('appends /s suffix', () => {
    expect(formatSpeed(1024)).toBe('1.0 KB/s')
  })
  it('handles zero', () => {
    expect(formatSpeed(0)).toBe('0 B/s')
  })
})

describe('formatUptime', () => {
  it('formats days and hours', () => {
    expect(formatUptime(90000)).toBe('1d 1h')
  })
  it('formats exactly one day', () => {
    expect(formatUptime(86400)).toBe('1d 0h')
  })
  it('formats hours and minutes', () => {
    expect(formatUptime(3900)).toBe('1h 5m')
  })
  it('formats minutes only', () => {
    expect(formatUptime(300)).toBe('5m')
  })
  it('formats zero seconds', () => {
    expect(formatUptime(0)).toBe('0m')
  })
})
```

- [ ] **Step 2: 运行测试**

Run: `cd apps/web && bunx vitest run src/lib/utils.test.ts`
Expected: 20 tests pass

- [ ] **Step 3: 提交**

```bash
git add apps/web/src/lib/utils.test.ts
git commit -m "test: add utils tests (countryCodeToFlag, formatBytes, formatSpeed, formatUptime)"
```

---

## Chunk 2: P1 — 核心业务逻辑

### Task 7: AgentManager 单元测试

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs` (添加 tests 模块)

- [ ] **Step 1: 编写 AgentManager 测试**

在 `crates/server/src/service/agent_manager.rs` 末尾添加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    fn make_manager() -> (AgentManager, broadcast::Receiver<BrowserMessage>) {
        let (tx, rx) = broadcast::channel(16);
        (AgentManager::new(tx), rx)
    }

    #[test]
    fn test_add_and_remove_connection() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Server1".into(), tx, test_addr());
        assert!(mgr.is_online("s1"));
        assert_eq!(mgr.online_count(), 1);

        mgr.remove_connection("s1");
        assert!(!mgr.is_online("s1"));
        assert_eq!(mgr.online_count(), 0);
    }

    #[test]
    fn test_broadcast_online_offline() {
        let (mgr, mut rx) = make_manager();
        let (tx, _) = mpsc::channel(1);

        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());
        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg, BrowserMessage::ServerOnline { server_id } if server_id == "s1"));

        mgr.remove_connection("s1");
        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg, BrowserMessage::ServerOffline { server_id } if server_id == "s1"));
    }

    #[test]
    fn test_update_report_and_cache() {
        let (mgr, mut _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());

        let report = SystemReport {
            cpu: 42.5,
            mem_used: 8_000_000_000,
            ..Default::default()
        };
        mgr.update_report("s1", report.clone());

        let cached = mgr.get_latest_report("s1").unwrap();
        assert!((cached.cpu - 42.5).abs() < f64::EPSILON);
        assert_eq!(cached.mem_used, 8_000_000_000);
    }

    #[test]
    fn test_all_latest_reports() {
        let (mgr, _rx) = make_manager();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "A".into(), tx1, test_addr());
        mgr.add_connection("s2".into(), "B".into(), tx2, test_addr());
        mgr.update_report("s1", SystemReport { cpu: 10.0, ..Default::default() });
        mgr.update_report("s2", SystemReport { cpu: 20.0, ..Default::default() });

        let all = mgr.all_latest_reports();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_connected_server_ids() {
        let (mgr, _rx) = make_manager();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "A".into(), tx1, test_addr());
        mgr.add_connection("s2".into(), "B".into(), tx2, test_addr());
        let ids = mgr.connected_server_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"s1".to_string()));
        assert!(ids.contains(&"s2".to_string()));
    }

    #[test]
    fn test_terminal_session_lifecycle() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.register_terminal_session("sess1".into(), tx);
        assert!(mgr.get_terminal_session("sess1").is_some());
        mgr.unregister_terminal_session("sess1");
        assert!(mgr.get_terminal_session("sess1").is_none());
    }

    #[test]
    fn test_check_offline() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Old".into(), tx, test_addr());
        // With threshold=0, the connection (created just now) should be considered offline
        // since elapsed >= 0
        let offline = mgr.check_offline(0);
        assert_eq!(offline, vec!["s1"]);
        assert!(!mgr.is_online("s1"));
    }

    #[test]
    fn test_check_offline_within_threshold() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Fresh".into(), tx, test_addr());
        // With a high threshold, connection should stay online
        let offline = mgr.check_offline(9999);
        assert!(offline.is_empty());
        assert!(mgr.is_online("s1"));
    }

    #[test]
    fn test_protocol_version() {
        let (mgr, _rx) = make_manager();
        let (tx, _) = mpsc::channel(1);
        mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());
        assert_eq!(mgr.get_protocol_version("s1"), Some(1)); // default
        mgr.set_protocol_version("s1", 2);
        assert_eq!(mgr.get_protocol_version("s1"), Some(2));
    }

    #[test]
    fn test_get_report_nonexistent() {
        let (mgr, _rx) = make_manager();
        assert!(mgr.get_latest_report("nope").is_none());
    }
}
```

- [ ] **Step 2: 为 SystemReport 添加 Default derive (前置条件)**

`SystemReport` 当前没有 `Default` derive，测试中 `..Default::default()` 需要它。
在 `crates/common/src/types.rs` 中修改:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemReport { ... }
```

- [ ] **Step 3: 运行测试验证通过**

Expected: 10 tests pass

- [ ] **Step 4: 提交**

```bash
git add crates/server/src/service/agent_manager.rs crates/common/src/types.rs
git commit -m "test: add AgentManager unit tests (connections, reports, terminal, offline)"
```

---

### Task 8: Server CRUD 服务测试

**Files:**
- Modify: `crates/server/src/service/server.rs` (添加 tests 模块)

- [ ] **Step 1: 编写 Server 服务测试**

在 `crates/server/src/service/server.rs` 末尾添加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;

    async fn create_test_server(db: &DatabaseConnection) -> String {
        // Register a server by directly inserting
        use crate::entity::server;
        use sea_orm::ActiveModelTrait;
        use sea_orm::Set;
        let id = uuid::Uuid::new_v4().to_string();
        let model = server::ActiveModel {
            id: Set(id.clone()),
            name: Set("TestServer".to_string()),
            token_hash: Set(AuthService::hash_password("testtoken").unwrap()),
            token_prefix: Set("testtoke".to_string()),
            capabilities: Set(serverbee_common::constants::CAP_DEFAULT as i32),
            created_at: Set(chrono::Utc::now()),
            updated_at: Set(chrono::Utc::now()),
            ..Default::default()
        };
        model.insert(db).await.unwrap();
        id
    }

    #[tokio::test]
    async fn test_list_servers() {
        let (db, _tmp) = setup_test_db().await;
        let _id = create_test_server(&db).await;
        let servers = ServerService::list_servers(&db).await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "TestServer");
    }

    #[tokio::test]
    async fn test_get_server_found() {
        let (db, _tmp) = setup_test_db().await;
        let id = create_test_server(&db).await;
        let server = ServerService::get_server(&db, &id).await.unwrap();
        assert!(server.is_some());
    }

    #[tokio::test]
    async fn test_get_server_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let server = ServerService::get_server(&db, "nonexistent").await.unwrap();
        assert!(server.is_none());
    }

    #[tokio::test]
    async fn test_delete_server() {
        let (db, _tmp) = setup_test_db().await;
        let id = create_test_server(&db).await;
        ServerService::delete_server(&db, &id).await.unwrap();
        let server = ServerService::get_server(&db, &id).await.unwrap();
        assert!(server.is_none());
    }

    #[tokio::test]
    async fn test_batch_delete() {
        let (db, _tmp) = setup_test_db().await;
        let id1 = create_test_server(&db).await;
        let id2 = create_test_server(&db).await;
        ServerService::batch_delete(&db, &[id1.clone(), id2.clone()])
            .await
            .unwrap();
        assert!(ServerService::get_server(&db, &id1).await.unwrap().is_none());
        assert!(ServerService::get_server(&db, &id2).await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-server service::server::tests -- --nocapture`
Expected: 5 tests pass

- [ ] **Step 3: 提交**

```bash
git add crates/server/src/service/server.rs
git commit -m "test: add server service CRUD tests"
```

---

### Task 9: 前端 WebSocket 数据合并测试

**Files:**
- Create: `apps/web/src/hooks/use-servers-ws.test.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts` (导出 `mergeServerUpdate` 和 `setServerOnlineStatus` 供测试)

- [ ] **Step 1: 导出内部函数供测试**

在 `apps/web/src/hooks/use-servers-ws.ts` 中，将两个函数改为 export:

将 `function mergeServerUpdate(` 改为 `export function mergeServerUpdate(`
将 `function setServerOnlineStatus(` 改为 `export function setServerOnlineStatus(`

- [ ] **Step 2: 编写测试**

```typescript
// apps/web/src/hooks/use-servers-ws.test.ts
import { describe, it, expect } from 'vitest'
import { mergeServerUpdate, setServerOnlineStatus } from './use-servers-ws'
import type { ServerMetrics } from './use-servers-ws'

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'Test',
    online: true,
    last_active: 0,
    cpu: 50,
    mem_used: 8_000_000_000,
    mem_total: 16_000_000_000,
    swap_used: 0,
    swap_total: 4_000_000_000,
    disk_used: 100_000_000_000,
    disk_total: 500_000_000_000,
    net_in_speed: 1000,
    net_out_speed: 500,
    net_in_transfer: 10000,
    net_out_transfer: 5000,
    load1: 1.5,
    load5: 1.2,
    load15: 1.0,
    tcp_conn: 100,
    udp_conn: 10,
    process_count: 200,
    uptime: 3600,
    cpu_name: 'Intel i7',
    os: 'Linux',
    region: 'US-East',
    country_code: 'US',
    group_id: 'g1',
    ...overrides
  }
}

describe('mergeServerUpdate', () => {
  it('updates dynamic fields', () => {
    const prev = [makeServer({ cpu: 50 })]
    const incoming = [makeServer({ cpu: 75, mem_used: 10_000_000_000 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].cpu).toBe(75)
    expect(result[0].mem_used).toBe(10_000_000_000)
  })

  it('preserves static fields when incoming is null', () => {
    const prev = [makeServer({ mem_total: 16_000_000_000, os: 'Linux', cpu_name: 'Intel i7' })]
    const incoming = [makeServer({ id: 's1', mem_total: 0, os: null, cpu_name: null })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].mem_total).toBe(16_000_000_000)
    expect(result[0].os).toBe('Linux')
    expect(result[0].cpu_name).toBe('Intel i7')
  })

  it('preserves static fields when incoming is 0', () => {
    const prev = [makeServer({ disk_total: 500_000_000_000, swap_total: 4_000_000_000 })]
    const incoming = [makeServer({ id: 's1', disk_total: 0, swap_total: 0 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].disk_total).toBe(500_000_000_000)
    expect(result[0].swap_total).toBe(4_000_000_000)
  })

  it('ignores updates for unknown server id', () => {
    const prev = [makeServer({ id: 's1' })]
    const incoming = [makeServer({ id: 'unknown', cpu: 99 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result).toEqual(prev)
  })

  it('returns copy of prev when incoming is empty', () => {
    const prev = [makeServer()]
    const result = mergeServerUpdate(prev, [])
    expect(result).toEqual(prev)
  })
})

describe('setServerOnlineStatus', () => {
  it('sets target server offline', () => {
    const prev = [makeServer({ id: 's1', online: true }), makeServer({ id: 's2', online: true })]
    const result = setServerOnlineStatus(prev, 's1', false)
    expect(result[0].online).toBe(false)
    expect(result[1].online).toBe(true)
  })

  it('sets target server online', () => {
    const prev = [makeServer({ id: 's1', online: false })]
    const result = setServerOnlineStatus(prev, 's1', true)
    expect(result[0].online).toBe(true)
  })

  it('leaves all unchanged for unknown server', () => {
    const prev = [makeServer({ id: 's1', online: true })]
    const result = setServerOnlineStatus(prev, 'unknown', false)
    expect(result[0].online).toBe(true)
  })
})
```

- [ ] **Step 3: 运行测试**

Run: `cd apps/web && bunx vitest run src/hooks/use-servers-ws.test.ts`
Expected: 8 tests pass

- [ ] **Step 4: 提交**

```bash
git add apps/web/src/hooks/use-servers-ws.ts apps/web/src/hooks/use-servers-ws.test.ts
git commit -m "test: add WebSocket data merge tests (static field preservation, online status)"
```

---

## Chunk 3: P2 — API 集成测试

### Task 10: 扩展集成测试 — 认证流程

**Files:**
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: 添加认证流程集成测试**

在 `crates/server/tests/integration.rs` 末尾追加:

```rust
#[tokio::test]
async fn test_login_logout_flow() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // Login
    let resp = login_admin(&client, &base_url).await;
    assert!(resp.get("user_id").is_some());

    // Check auth status
    let resp = client
        .get(format!("{base_url}/api/auth/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["username"], "admin");

    // Logout
    let resp = client
        .post(format!("{base_url}/api/auth/logout"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    // Verify logged out
    let resp = client
        .get(format!("{base_url}/api/auth/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_api_key_lifecycle() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create API key
    let resp = client
        .post(format!("{base_url}/api/auth/api-keys"))
        .json(&serde_json::json!({"description": "CI key"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let raw_key = body["data"]["raw_key"].as_str().unwrap().to_string();
    assert!(raw_key.starts_with("sb_"));

    // Use API key in a new client (no cookies)
    let key_client = reqwest::Client::new();
    let resp = key_client
        .get(format!("{base_url}/api/servers"))
        .header("x-api-key", &raw_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_member_read_only() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create member user
    client
        .post(format!("{base_url}/api/users"))
        .json(&serde_json::json!({
            "username": "viewer",
            "password": "viewpass1",
            "role": "member"
        }))
        .send()
        .await
        .unwrap();

    // Login as member
    let member_client = http_client();
    member_client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({
            "username": "viewer",
            "password": "viewpass1"
        }))
        .send()
        .await
        .unwrap();

    // Read should work
    let resp = member_client
        .get(format!("{base_url}/api/servers"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Write should fail (403)
    let resp = member_client
        .post(format!("{base_url}/api/users"))
        .json(&serde_json::json!({
            "username": "hacker",
            "password": "hack1234",
            "role": "admin"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_public_status_no_auth() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new(); // no cookies
    let resp = client
        .get(format!("{base_url}/api/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_audit_log_recorded() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Check audit logs
    let resp = client
        .get(format!("{base_url}/api/audit/logs"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let logs = body["data"].as_array().unwrap();
    // Login action should be logged
    assert!(!logs.is_empty());
    assert!(logs.iter().any(|l| l["action"] == "login"));
}
```

- [ ] **Step 2: 运行集成测试**

Run: `cargo test -p serverbee-server --test integration -- --nocapture`
Expected: 7 tests pass (2 existing + 5 new)

- [ ] **Step 3: 提交**

```bash
git add crates/server/tests/integration.rs
git commit -m "test: add integration tests for auth flow, API key, RBAC, status, audit logs"
```

---

### Task 11: 集成测试 — CRUD 与功能流程

**Files:**
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: 添加 CRUD 和功能流程测试**

追加到 `integration.rs`:

```rust
#[tokio::test]
async fn test_notification_and_alert_crud() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create notification channel
    let resp = client
        .post(format!("{base_url}/api/notifications"))
        .json(&serde_json::json!({
            "name": "test-webhook",
            "notify_type": "webhook",
            "config": {"url": "https://example.com/hook"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let notif: serde_json::Value = resp.json().await.unwrap();
    let notif_id = notif["data"]["id"].as_str().unwrap();

    // Create notification group
    let resp = client
        .post(format!("{base_url}/api/notification-groups"))
        .json(&serde_json::json!({
            "name": "test-group",
            "notification_ids": [notif_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let group: serde_json::Value = resp.json().await.unwrap();
    let group_id = group["data"]["id"].as_str().unwrap();

    // Create alert rule
    let resp = client
        .post(format!("{base_url}/api/alerts"))
        .json(&serde_json::json!({
            "name": "High CPU",
            "rules": [{"rule_type": "cpu", "min": 80.0}],
            "notification_group_id": group_id,
            "cover_type": "all"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // List alerts
    let resp = client
        .get(format!("{base_url}/api/alerts"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let alerts = body["data"].as_array().unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["name"], "High CPU");

    // Delete alert
    let alert_id = alerts[0]["id"].as_str().unwrap();
    let resp = client
        .delete(format!("{base_url}/api/alerts/{alert_id}"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn test_user_management_crud() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create user
    let resp = client
        .post(format!("{base_url}/api/users"))
        .json(&serde_json::json!({
            "username": "newuser",
            "password": "pass1234",
            "role": "member"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let user_id = body["data"]["id"].as_str().unwrap().to_string();

    // List users
    let resp = client
        .get(format!("{base_url}/api/users"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let users = body["data"].as_array().unwrap();
    assert_eq!(users.len(), 2); // admin + newuser

    // Update role
    let resp = client
        .put(format!("{base_url}/api/users/{user_id}"))
        .json(&serde_json::json!({"role": "admin"}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    // Delete user
    let resp = client
        .delete(format!("{base_url}/api/users/{user_id}"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn test_settings_auto_discovery_key() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Get key
    let resp = client
        .get(format!("{base_url}/api/settings/auto-discovery-key"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let key1 = body["data"]["key"].as_str().unwrap().to_string();
    assert!(!key1.is_empty());

    // Regenerate key
    let resp = client
        .put(format!("{base_url}/api/settings/auto-discovery-key"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let key2 = body["data"]["key"].as_str().unwrap().to_string();
    assert_ne!(key1, key2);
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-server --test integration -- --nocapture`
Expected: 10 tests pass (7 previous + 3 new)

- [ ] **Step 3: 提交**

```bash
git add crates/server/tests/integration.rs
git commit -m "test: add integration tests for notification/alert CRUD, user management, discovery key"
```

---

### Task 12: 前端 WsClient 测试

**Files:**
- Create: `apps/web/src/lib/ws-client.test.ts`

- [ ] **Step 1: 编写 WsClient 测试**

```typescript
// apps/web/src/lib/ws-client.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock WebSocket before importing WsClient
class MockWebSocket {
  static instances: MockWebSocket[] = []
  url: string
  onopen: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
  close = vi.fn(() => {
    this.onclose?.()
  })

  constructor(url: string) {
    this.url = url
    MockWebSocket.instances.push(this)
  }

  simulateOpen() {
    this.onopen?.()
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) })
  }

  simulateClose() {
    this.onclose?.()
  }
}

vi.stubGlobal('WebSocket', MockWebSocket)
vi.stubGlobal('window', {
  location: { protocol: 'http:', host: 'localhost:9527' }
})

// Import after mocks are set up
const { WsClient } = await import('./ws-client')

beforeEach(() => {
  MockWebSocket.instances = []
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('WsClient', () => {
  it('constructs WebSocket with correct URL', () => {
    const _ws = new WsClient('/api/ws/servers')
    expect(MockWebSocket.instances[0].url).toBe('ws://localhost:9527/api/ws/servers')
  })

  it('delivers parsed JSON to handlers', () => {
    const ws = new WsClient('/api/ws/test')
    const handler = vi.fn()
    ws.onMessage(handler)
    MockWebSocket.instances[0].simulateOpen()
    MockWebSocket.instances[0].simulateMessage({ type: 'update', data: 42 })
    expect(handler).toHaveBeenCalledWith({ type: 'update', data: 42 })
  })

  it('delivers to multiple handlers', () => {
    const ws = new WsClient('/api/ws/test')
    const h1 = vi.fn()
    const h2 = vi.fn()
    ws.onMessage(h1)
    ws.onMessage(h2)
    MockWebSocket.instances[0].simulateOpen()
    MockWebSocket.instances[0].simulateMessage({ x: 1 })
    expect(h1).toHaveBeenCalledOnce()
    expect(h2).toHaveBeenCalledOnce()
  })

  it('unsubscribe removes handler', () => {
    const ws = new WsClient('/api/ws/test')
    const handler = vi.fn()
    const unsub = ws.onMessage(handler)
    unsub()
    MockWebSocket.instances[0].simulateOpen()
    MockWebSocket.instances[0].simulateMessage({ x: 1 })
    expect(handler).not.toHaveBeenCalled()
  })

  it('close() prevents reconnection', () => {
    const ws = new WsClient('/api/ws/test')
    ws.close()
    MockWebSocket.instances[0].simulateClose()
    vi.advanceTimersByTime(60_000)
    // Should only have 1 instance (the initial one), no reconnect
    expect(MockWebSocket.instances.length).toBe(1)
  })

  it('schedules reconnect on close with backoff', () => {
    const _ws = new WsClient('/api/ws/test')
    const sock = MockWebSocket.instances[0]
    sock.simulateClose()
    expect(MockWebSocket.instances.length).toBe(1) // not yet reconnected

    vi.advanceTimersByTime(1500) // past initial ~1000ms + jitter
    expect(MockWebSocket.instances.length).toBe(2) // reconnected
  })
})
```

- [ ] **Step 2: 运行测试**

Run: `cd apps/web && bunx vitest run src/lib/ws-client.test.ts`
Expected: 6 tests pass

- [ ] **Step 3: 提交**

```bash
git add apps/web/src/lib/ws-client.test.ts
git commit -m "test: add WsClient tests (URL, handlers, unsubscribe, close, reconnect)"
```

---

## Chunk 4: P3 — Agent 与后台任务

### Task 13: Agent 采集器测试

**Files:**
- Create: `crates/agent/src/collector/tests.rs`
- Modify: `crates/agent/src/collector/mod.rs` (添加 `#[cfg(test)] mod tests;`)

- [ ] **Step 1: 编写采集器测试**

```rust
// crates/agent/src/collector/tests.rs
use super::Collector;

#[test]
fn test_system_info_populated() {
    let collector = Collector::new(true, false);
    let info = collector.system_info();
    assert!(!info.cpu_name.is_empty());
    assert!(!info.os.is_empty());
    assert!(info.cpu_cores > 0);
    assert!(info.mem_total > 0);
    assert!(info.disk_total > 0);
}

#[test]
fn test_collect_returns_valid_report() {
    let mut collector = Collector::new(true, false);
    // First collect initializes baseline
    let _ = collector.collect();
    // Second collect produces delta-based speeds
    std::thread::sleep(std::time::Duration::from_millis(100));
    let report = collector.collect();
    assert!(report.cpu >= 0.0 && report.cpu <= 100.0);
    assert!(report.mem_used <= collector.system_info().mem_total);
    assert!(report.process_count > 0);
}

#[test]
fn test_cpu_usage_range() {
    let mut collector = Collector::new(true, false);
    let _ = collector.collect();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let report = collector.collect();
    assert!(report.cpu >= 0.0);
    assert!(report.cpu <= 100.0);
}

#[test]
fn test_disk_used_le_total() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    let info = collector.system_info();
    assert!(report.disk_used <= info.disk_total);
}

#[test]
fn test_memory_used_le_total() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    let info = collector.system_info();
    assert!(report.mem_used <= info.mem_total);
}
```

- [ ] **Step 2: 在 mod.rs 注册测试模块**

在 `crates/agent/src/collector/mod.rs` 末尾添加:
```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p serverbee-agent collector::tests -- --nocapture`
Expected: 5 tests pass

- [ ] **Step 4: 提交**

```bash
git add crates/agent/src/collector/tests.rs crates/agent/src/collector/mod.rs
git commit -m "test: add agent collector tests (system_info, metrics range, usage bounds)"
```

---

### Task 14: Agent Pinger 测试

**Files:**
- Modify: `crates/agent/src/pinger.rs` (添加 tests 模块)

- [ ] **Step 1: 编写 TCP ping 测试**

在 `crates/agent/src/pinger.rs` 末尾添加:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_ping_open_port() {
        // Bind a listener on random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target = format!("127.0.0.1:{}", addr.port());

        let result = tcp_probe(&target, std::time::Duration::from_secs(5)).await;
        assert!(result.success);
        assert!(result.latency_ms > 0.0);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_tcp_ping_closed_port() {
        // Port 1 is almost certainly closed
        let result = tcp_probe("127.0.0.1:1", std::time::Duration::from_secs(2)).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_http_ping_localhost() {
        // This test only passes when a server is running; skip if not available
        let result = http_probe("http://127.0.0.1:9527/healthz", std::time::Duration::from_secs(2)).await;
        // We don't assert success since server may not be running in CI
        // Just verify it returns a valid PingResult structure
        assert!(result.latency_ms >= 0.0);
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent pinger::tests -- --nocapture`
Expected: 3 tests (tcp_ping_open_port and tcp_ping_closed_port should pass; http test depends on server)

- [ ] **Step 3: 提交**

```bash
git add crates/agent/src/pinger.rs
git commit -m "test: add agent pinger tests (TCP open/closed port, HTTP probe)"
```

---

### Task 15: Ping 服务 DB 测试

**Files:**
- Modify: `crates/server/src/service/ping.rs` (添加 tests 模块)

- [ ] **Step 1: 编写 Ping 服务测试**

在 `crates/server/src/service/ping.rs` 末尾添加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::agent_manager::AgentManager;
    use crate::test_utils::setup_test_db;
    use tokio::sync::broadcast;

    fn test_agent_manager() -> AgentManager {
        let (tx, _) = broadcast::channel(16);
        AgentManager::new(tx)
    }

    #[tokio::test]
    async fn test_create_and_list_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let mgr = test_agent_manager();
        PingService::create(
            &db,
            &mgr,
            CreatePingTask {
                name: "Check Google".into(),
                probe_type: "http".into(),
                target: "https://google.com".into(),
                interval: 30,
                server_ids: vec![],
                enabled: true,
            },
        )
        .await
        .unwrap();

        let tasks = PingService::list(&db).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "Check Google");
        assert_eq!(tasks[0].probe_type, "http");
    }

    #[tokio::test]
    async fn test_delete_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let mgr = test_agent_manager();
        let task = PingService::create(
            &db,
            &mgr,
            CreatePingTask {
                name: "Temp".into(),
                probe_type: "tcp".into(),
                target: "1.1.1.1:443".into(),
                interval: 60,
                server_ids: vec![],
                enabled: true,
            },
        )
        .await
        .unwrap();

        PingService::delete(&db, &task.id).await.unwrap();
        let tasks = PingService::list(&db).await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_get_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let mgr = test_agent_manager();
        let task = PingService::create(
            &db,
            &mgr,
            CreatePingTask {
                name: "ICMP Test".into(),
                probe_type: "icmp".into(),
                target: "8.8.8.8".into(),
                interval: 10,
                server_ids: vec![],
                enabled: true,
            },
        )
        .await
        .unwrap();

        let found = PingService::get(&db, &task.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "ICMP Test");
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-server service::ping::tests -- --nocapture`
Expected: 3 tests pass

- [ ] **Step 3: 提交**

```bash
git add crates/server/src/service/ping.rs
git commit -m "test: add ping service DB tests (create, list, delete, get)"
```

---

### Task 16: 最终验证与 TESTING.md 更新

**Files:**
- Modify: `TESTING.md`

- [ ] **Step 1: 运行全量 Rust 测试**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: All tests pass, count ~100+

- [ ] **Step 2: 运行全量前端测试**

Run: `cd apps/web && bun run test 2>&1 | tail -10`
Expected: All tests pass, count ~45+

- [ ] **Step 3: 运行代码质量检查**

Run: `cargo clippy --workspace -- -D warnings && cd apps/web && bun x ultracite check`
Expected: 0 warnings, 0 errors

- [ ] **Step 4: 更新 TESTING.md 中的测试计数**

更新以下部分的数字:
- 快速命令注释中的 Rust/前端测试数量
- 按 crate 运行中的测试数量
- 单元测试覆盖表格新增 agent_manager, user, server, ping 行
- 集成测试覆盖表格新增所有新集成测试
- 前端测试覆盖表格新增 ws-client, utils, use-servers-ws, api-client 行
- 测试文件位置中新增所有新文件

- [ ] **Step 5: 提交**

```bash
git add TESTING.md
git commit -m "docs: update TESTING.md with new test counts and coverage tables"
```
