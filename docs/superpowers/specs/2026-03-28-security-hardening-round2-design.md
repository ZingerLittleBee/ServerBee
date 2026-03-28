# Security Hardening Round 2 — Design Spec

> Date: 2026-03-28

## Overview

Round 1 (seattle branch, v0.7.x) fixed 8 critical/high issues. This round addresses 5 remaining items identified in the 2026-03-26 code review: 3 confirmed issues + 2 quality optimizations.

## Scope

| # | Issue | Severity | Category |
|---|-------|----------|----------|
| F1 | `batch_update_capabilities` 无事务保护 | High | Data consistency |
| F2 | 缺少安全响应头 | Medium | Security hardening |
| F3 | 文件上传无服务端大小限制 | Medium | DoS prevention |
| F4 | `/api/auth/me` 每次调用做 argon2 验证 | Low | Performance |
| F5 | `fetch_external_ip` 响应无大小限制 | Low | DoS prevention |

## F1: batch_update_capabilities 事务保护

### Problem

`batch_update_capabilities` (server.rs:602-722) 循环更新多个 server 的 capabilities，每次直接调用 `active.update(&state.db)`。中途失败会导致前 N 个 server 已写入 DB，后续 server 未更新，API 返回错误但数据已部分修改。

### Solution

将 DB 操作包裹在事务中，副作用（WS 广播、审计日志、Docker 清理）移到 commit 之后执行。

### Design

```
let txn = state.db.begin().await?;

// Phase 1: DB updates in transaction
let mut side_effects: Vec<CapabilityChangeEffect> = Vec::new();
for s in &servers {
    // compute new_caps...
    active.update(&txn).await?;
    side_effects.push(CapabilityChangeEffect {
        server_id, old_caps, new_caps, is_online, protocol_version
    });
}
txn.commit().await?;

// Phase 2: side effects (fire-and-forget, idempotent)
for effect in side_effects {
    // WS broadcast, audit log, Docker cleanup, ping re-sync
}
```

### Files Changed

- `crates/server/src/router/api/server.rs` — batch_update_capabilities

### Notes

- 副作用失败不影响 DB 一致性（已 commit）
- 副作用本身都是幂等的（WS 广播、审计日志）
- 定义 `CapabilityChangeEffect` struct 收集需要执行的副作用

## F2: 安全响应头

### Problem

`create_router` (router/mod.rs) 仅有 `TraceLayer`，无任何安全响应头。

### Solution

使用 `tower_http::set_header::SetResponseHeaderLayer` 添加 4 个标准安全头。

### Headers

| Header | Value | Purpose |
|--------|-------|---------|
| X-Frame-Options | DENY | 防止 clickjacking |
| X-Content-Type-Options | nosniff | 防止 MIME 嗅探 |
| Referrer-Policy | strict-origin-when-cross-origin | 控制 referer 泄漏 |
| X-Permitted-Cross-Domain-Policies | none | 阻止 Flash/PDF 跨域 |

### Not Included

- **CSP**: SPA 内联 style (shadcn/ui) + 动态主题 CSS + Monaco Editor 需要大量 unsafe-inline 例外，维护成本高于收益
- **HSTS**: 用户可能不走 HTTPS（自托管场景），HSTS 会锁死浏览器，交给反代处理
- **Permissions-Policy**: 监控工具不涉及摄像头/麦克风等敏感 API

### Files Changed

- `crates/server/Cargo.toml` — tower-http features 添加 `"set-header"`
- `crates/server/src/router/mod.rs` — create_router 添加 4 个 SetResponseHeaderLayer

## F3: 文件上传大小限制

### Problem

`upload_file` handler (file.rs:832-895) 流式写入临时文件时不检查累积大小。虽然 Axum Multipart 有默认限制，但未显式配置，且默认值可能随版本变化。

### Solution

1. Server 新增 `file.max_upload_size` 配置项（默认 100MB）
2. 在 chunk 循环中累积检查 `file_size`，超限立即中断并清理临时文件
3. 给文件上传路由设置显式 `DefaultBodyLimit`

### Design

```rust
// config.rs
pub struct FileConfig {
    pub max_upload_size: u64,  // default 104_857_600 (100MB)
}

// file.rs upload_file handler, chunk loop:
file_size += chunk.len() as u64;
if file_size > state.config.file.max_upload_size {
    let _ = tokio::fs::remove_file(&temp_upload).await;
    return Err(AppError::BadRequest(format!(
        "File size exceeds limit of {} bytes",
        state.config.file.max_upload_size
    )));
}
```

### Files Changed

- `crates/server/src/config.rs` — 新增 `FileConfig` struct
- `crates/server/src/router/api/file.rs` — upload_file 添加大小检查 + write_router 添加 DefaultBodyLimit
- `ENV.md` — 文档更新

### Notes

- `DefaultBodyLimit` 设为 `max_upload_size + 1MB`（留余给 multipart metadata）
- Agent 端已有 `max_file_size` 配置（默认 1GB），Server 端限制是第一道防线

## F4: /api/auth/me argon2 性能优化

### Problem

`/api/auth/me` (auth.rs:247-271) 每次调用对 admin 用户执行 `AuthService::verify_password("admin", &user.password_hash)`，argon2 验证耗时 ~10-50ms。

### Solution

在 session 表新增 `must_change_password` 字段，登录时计算一次并写入 session，`/me` 直接读 session 中的缓存值。修改密码时清除所有 admin session 的该标记。

### Design

**Database migration:**
```sql
ALTER TABLE sessions ADD COLUMN must_change_password BOOLEAN NOT NULL DEFAULT FALSE;
```

**Login flow (auth.rs login handler):**
```rust
// After successful login, before creating session:
let must_change_password = if user.username == "admin" {
    AuthService::verify_password("admin", &user.password_hash).unwrap_or(false)
} else {
    false
};
// Store in session record
```

**Me endpoint:**
```rust
// Read from session directly, no argon2 call
ok(MeResponse {
    must_change_password: current_user.must_change_password,
    ..
})
```

**Change password:**
```rust
// After password change succeeds, update all admin sessions:
// UPDATE sessions SET must_change_password = false WHERE user_id = ?
```

### Files Changed

- `crates/server/src/migration/` — 新增 migration 添加 `must_change_password` 列
- `crates/server/src/entity/session.rs` — 新增字段
- `crates/server/src/service/auth.rs` — create_session 接受 must_change_password 参数；validate_session 读取并传递到 CurrentUser
- `crates/server/src/router/api/auth.rs` — login 计算 + me 读缓存 + change_password 清除标记
- `crates/server/src/middleware/auth.rs` — CurrentUser 新增 must_change_password 字段

## F5: fetch_external_ip 响应大小限制

### Problem

`fetch_external_ip` (reporter.rs:1576-1583) 使用 `resp.text()` 无大小限制，恶意 IP 服务可返回巨量数据导致 OOM。

### Solution

检查 `Content-Length` header + 截断读取，限制 256 字节。

### Design

```rust
async fn fetch_external_ip(url: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let resp = client.get(url).send().await?;

    // Reject responses claiming to be larger than 256 bytes
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

### Files Changed

- `crates/agent/src/reporter.rs` — fetch_external_ip

## Testing Strategy

| Fix | Test Method |
|-----|-------------|
| F1 | 已有集成测试覆盖 capabilities API，验证事务后行为不变 |
| F2 | 新增 1 个集成测试：GET /healthz 检查响应头存在 |
| F3 | 新增 1 个单元测试：验证超限上传被拒绝 |
| F4 | 已有 auth 集成测试，验证 login → me → change_password 流程 |
| F5 | 现有 agent 测试覆盖，改动极小无需新增 |

## Out of Scope

以下问题在 review 中确认为非问题，不在本次修复范围：

- 文件读取 Member 权限（设计意图）
- queryHash 硬编码（代码质量，非安全）
- Custom CSS XSS（Admin-only）
- Widget 类型断言（代码质量，非安全）
- Upload chunk dead code（预留字段）
- Traceroute 下划线拒绝（刻意安全设计）
- HTTP 探测 accept_invalid_certs（探测器设计）
