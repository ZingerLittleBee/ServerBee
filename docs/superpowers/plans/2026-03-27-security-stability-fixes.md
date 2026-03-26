# Security & Stability Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 8 issues from the 2026-03-26 code review report — 4 security-critical and 4 stability items.

**Architecture:** Server-side fixes span config, router, state, service, and migration layers. Agent-side touches reporter and file_manager. Frontend touches two WS hook files. Common crate gets a protocol change. Release pipeline adds checksum generation.

**Tech Stack:** Rust (Axum 0.8, sea-orm, tokio, ipnet), TypeScript (React hooks), SQLite, GitHub Actions

**Spec:** `docs/superpowers/specs/2026-03-26-security-stability-fixes-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/server/Cargo.toml` | Modify | Add `ipnet` dependency |
| `crates/server/src/config.rs` | Modify | Add `trusted_proxies` to `ServerConfig`, add `UpgradeConfig` |
| `crates/server/src/router/mod.rs` | Modify | Remove CorsLayer |
| `crates/server/src/router/utils.rs` | Create | Unified `extract_client_ip` |
| `crates/server/src/router/api/auth.rs` | Modify | Use shared `extract_client_ip` |
| `crates/server/src/router/api/file.rs` | Modify | Use shared `extract_client_ip` |
| `crates/server/src/router/api/server.rs` | Modify | Use shared `extract_client_ip` + rewrite `trigger_upgrade` |
| `crates/server/src/router/api/agent.rs` | Modify | Use shared `extract_client_ip` |
| `crates/server/src/router/api/oauth.rs` | Modify | Use shared `extract_client_ip` |
| `crates/server/src/router/ws/agent.rs` | Modify | Token from header + max_message_size |
| `crates/server/src/router/ws/browser.rs` | Modify | max_message_size |
| `crates/server/src/router/ws/terminal.rs` | Modify | max_message_size |
| `crates/server/src/router/ws/docker_logs.rs` | Modify | max_message_size |
| `crates/server/src/state.rs` | Modify | DashMap expiry cleanup |
| `crates/server/src/service/record.rs` | Modify | Rewrite `aggregate_hourly` |
| `crates/server/src/service/agent_manager.rs` | Modify | Add `os`/`arch` to `AgentConnection` |
| `crates/server/src/migration/mod.rs` | Modify | Register new migration |
| `crates/server/src/migration/m20260327_000012_records_hourly_unique.rs` | Create | Dedup + unique index |
| `crates/common/src/protocol.rs` | Modify | `ServerMessage::Upgrade` add `sha256` |
| `crates/agent/src/reporter.rs` | Modify | WS header auth + `perform_upgrade` rewrite |
| `crates/agent/src/file_manager.rs` | Modify | `getpwuid_r`/`getgrgid_r` |
| `apps/web/src/hooks/use-terminal-ws.ts` | Modify | try/catch + base64 guard |
| `apps/web/src/hooks/use-servers-ws.ts` | Modify | Two-layer runtime validation |
| `.github/workflows/release.yml` | Modify | checksums.txt generation |

---

## Task 1: Remove CORS

**Files:**
- Modify: `crates/server/src/router/mod.rs`

- [ ] **Step 1: Remove CorsLayer from router**

In `crates/server/src/router/mod.rs`, remove the CORS import and layer:

```rust
// DELETE these lines:
use tower_http::cors::{Any, CorsLayer};

// DELETE these lines (around line 17-20):
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any);

// DELETE .layer(cors) from the Router chain (around line 38)
```

Also remove `"cors"` from `tower-http` features in `crates/server/Cargo.toml` if no other code uses it.

- [ ] **Step 2: Verify build**

Run: `cargo build -p serverbee-server`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/mod.rs crates/server/Cargo.toml
git commit -m "fix(security): remove permissive CORS policy

SPA is embedded via rust-embed, always same-origin. No CORS needed."
```

---

## Task 2: WebSocket Message Size Limits

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/router/ws/browser.rs`
- Modify: `crates/server/src/router/ws/terminal.rs`
- Modify: `crates/server/src/router/ws/docker_logs.rs`

- [ ] **Step 1: Add max_message_size to all WS handlers**

In each of the 4 WS handler files, find the `ws.on_upgrade(...)` call and chain `.max_message_size(MAX_WS_MESSAGE_SIZE)` before it.

Add the import at the top of each file:
```rust
use serverbee_common::constants::MAX_WS_MESSAGE_SIZE;
```

Then modify the upgrade call. Example for `agent.rs`:
```rust
// Before:
Ok(ws.on_upgrade(move |socket| handle_agent_ws(socket, state, server_id, remote_addr)))

// After:
Ok(ws
    .max_message_size(MAX_WS_MESSAGE_SIZE)
    .on_upgrade(move |socket| handle_agent_ws(socket, state, server_id, remote_addr)))
```

Apply the same pattern to `browser.rs`, `terminal.rs`, and `docker_logs.rs`.

Note: Check the actual Axum 0.8 API — it may be `.max_message_size(Some(MAX_WS_MESSAGE_SIZE))` with an `Option<usize>`. Verify with `cargo doc -p axum --open` or the axum source if needed.

- [ ] **Step 2: Verify build**

Run: `cargo build -p serverbee-server`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/ws/
git commit -m "fix(security): enforce MAX_WS_MESSAGE_SIZE on all WebSocket handlers

The 1MB limit constant was defined but never applied. Now configured
on all 4 WS upgrade points (agent, browser, terminal, docker_logs)."
```

---

## Task 3: Config + ipnet Dependency

**Files:**
- Modify: `crates/server/Cargo.toml`
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Add ipnet dependency**

In `crates/server/Cargo.toml`, add under `[dependencies]`:
```toml
ipnet = "2"
```

- [ ] **Step 2: Add trusted_proxies to ServerConfig**

In `crates/server/src/config.rs`, modify `ServerConfig`:

```rust
use ipnet::IpNet;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default)]
    pub trusted_proxies: Vec<IpNet>,
}
```

- [ ] **Step 3: Add UpgradeConfig**

In the same file, add a new config struct and register it in `AppConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct UpgradeConfig {
    #[serde(default = "default_release_base_url")]
    pub release_base_url: String,
}

fn default_release_base_url() -> String {
    "https://github.com/ZingerLittleBee/ServerBee/releases".to_string()
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            release_base_url: default_release_base_url(),
        }
    }
}
```

Add to `AppConfig`:
```rust
pub struct AppConfig {
    // ... existing fields ...
    #[serde(default)]
    pub upgrade: UpgradeConfig,
}
```

- [ ] **Step 4: Verify build**

Run: `cargo build -p serverbee-server`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add crates/server/Cargo.toml crates/server/src/config.rs
git commit -m "feat(config): add trusted_proxies and upgrade config

- ServerConfig.trusted_proxies: CIDR list for reverse proxy trust
- UpgradeConfig.release_base_url: configurable release source"
```

---

## Task 4: Unified extract_client_ip

**Files:**
- Create: `crates/server/src/router/utils.rs`
- Modify: `crates/server/src/router/mod.rs` (add `pub mod utils;`)
- Modify: `crates/server/src/router/api/auth.rs`
- Modify: `crates/server/src/router/api/file.rs`
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/router/api/agent.rs`
- Modify: `crates/server/src/router/api/oauth.rs`

- [ ] **Step 1: Write tests for extract_client_ip**

Create `crates/server/src/router/utils.rs`:

```rust
use axum::extract::ConnectInfo;
use http::HeaderMap;
use ipnet::IpNet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Extract the real client IP, respecting trusted proxy configuration.
///
/// If the TCP source IP is in `trusted_proxies`, parse `X-Forwarded-For` from
/// right to left and return the first IP not in the trusted set.
/// Otherwise, return the TCP source IP directly.
pub fn extract_client_ip(
    connect_info: &ConnectInfo<SocketAddr>,
    headers: &HeaderMap,
    trusted_proxies: &[IpNet],
) -> IpAddr {
    let tcp_ip = connect_info.0.ip();

    if trusted_proxies.is_empty() || !is_trusted(tcp_ip, trusted_proxies) {
        return tcp_ip;
    }

    // Parse X-Forwarded-For, right to left, find first non-trusted IP
    if let Some(xff) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        let ips: Vec<&str> = xff.split(',').map(|s| s.trim()).collect();
        for raw in ips.iter().rev() {
            if let Ok(ip) = raw.parse::<IpAddr>() {
                if !is_trusted(ip, trusted_proxies) {
                    return ip;
                }
            }
        }
    }

    // Fallback: try X-Real-IP
    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<IpAddr>().ok())
    {
        return real_ip;
    }

    tcp_ip
}

fn is_trusted(ip: IpAddr, trusted_proxies: &[IpNet]) -> bool {
    trusted_proxies.iter().any(|net| net.contains(&ip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn connect(ip: &str) -> ConnectInfo<SocketAddr> {
        ConnectInfo(SocketAddr::new(ip.parse().unwrap(), 12345))
    }

    fn headers_with_xff(xff: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", xff.parse().unwrap());
        h
    }

    #[test]
    fn no_trusted_proxies_returns_tcp_ip() {
        let ci = connect("1.2.3.4");
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "9.9.9.9".parse().unwrap());
        let ip = extract_client_ip(&ci, &h, &[]);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn trusted_proxy_reads_xff_rightmost_untrusted() {
        let ci = connect("127.0.0.1");
        let trusted: Vec<IpNet> = vec!["127.0.0.0/8".parse().unwrap()];
        let h = headers_with_xff("9.9.9.9, 10.0.0.1, 127.0.0.1");
        let ip = extract_client_ip(&ci, &h, &trusted);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
    }

    #[test]
    fn trusted_proxy_skips_all_trusted_in_chain() {
        let ci = connect("10.0.0.1");
        let trusted: Vec<IpNet> = vec![
            "10.0.0.0/8".parse().unwrap(),
            "127.0.0.0/8".parse().unwrap(),
        ];
        let h = headers_with_xff("1.2.3.4, 10.0.0.2, 10.0.0.1");
        let ip = extract_client_ip(&ci, &h, &trusted);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn untrusted_source_ignores_xff() {
        let ci = connect("8.8.8.8");
        let trusted: Vec<IpNet> = vec!["127.0.0.0/8".parse().unwrap()];
        let h = headers_with_xff("1.1.1.1");
        let ip = extract_client_ip(&ci, &h, &trusted);
        // 8.8.8.8 is not trusted, so XFF is ignored
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)));
    }

    #[test]
    fn no_xff_header_returns_tcp_ip() {
        let ci = connect("127.0.0.1");
        let trusted: Vec<IpNet> = vec!["127.0.0.0/8".parse().unwrap()];
        let ip = extract_client_ip(&ci, &HeaderMap::new(), &trusted);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn spoofed_xff_from_untrusted_client_ignored() {
        let ci = connect("203.0.113.50");
        let trusted: Vec<IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let h = headers_with_xff("192.168.1.1");
        let ip = extract_client_ip(&ci, &h, &trusted);
        // Client 203.0.113.50 is not a trusted proxy, XFF ignored
        assert_eq!(ip, "203.0.113.50".parse::<IpAddr>().unwrap());
    }
}
```

- [ ] **Step 2: Register module and run tests**

Add `pub mod utils;` to `crates/server/src/router/mod.rs`.

Run: `cargo test -p serverbee-server router::utils`
Expected: All 6 tests pass

- [ ] **Step 3: Replace extract_client_ip in auth.rs**

In `crates/server/src/router/api/auth.rs`:

1. Delete the local `fn extract_client_ip(headers: &HeaderMap) -> String` function (lines 594-606)
2. Add `ConnectInfo<SocketAddr>` extractor to `login` handler signature
3. Replace `let ip = extract_client_ip(&req_headers);` with:

```rust
use crate::router::utils::extract_client_ip;
use axum::extract::ConnectInfo;
use std::net::SocketAddr;

// In the handler signature, add:
//   ConnectInfo(addr): ConnectInfo<SocketAddr>,

let ip = extract_client_ip(
    &ConnectInfo(addr),
    &req_headers,
    &state.config.server.trusted_proxies,
).to_string();
```

Apply same pattern to `register` handler if it also uses `extract_client_ip`.

- [ ] **Step 4: Replace extract_client_ip in file.rs, server.rs, agent.rs**

Same pattern as Step 3. Delete local `extract_client_ip` function, import from `crate::router::utils`, add `ConnectInfo<SocketAddr>` extractor where needed.

- [ ] **Step 5: Replace inline IP extraction in oauth.rs**

In `crates/server/src/router/api/oauth.rs`, replace lines 186-191:

```rust
// DELETE:
let ip = headers
    .get("x-forwarded-for")
    .and_then(|v| v.to_str().ok())
    .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
    .unwrap_or_else(|| "unknown".to_string());

// REPLACE WITH:
use crate::router::utils::extract_client_ip;
let ip = extract_client_ip(
    &ConnectInfo(addr),
    &headers,
    &state.config.server.trusted_proxies,
).to_string();
```

Add `ConnectInfo(addr): ConnectInfo<SocketAddr>` to `oauth_callback` handler signature.

- [ ] **Step 6: Verify build and tests**

Run: `cargo build -p serverbee-server && cargo test -p serverbee-server`
Expected: builds and all tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/router/
git commit -m "fix(security): unify extract_client_ip with trusted proxy support

- New shared extract_client_ip in router/utils.rs
- Respects server.trusted_proxies CIDR config
- Replaces 5 duplicated implementations (auth, file, server, agent, oauth)
- XFF only trusted when TCP source is a known proxy"
```

---

## Task 5: DashMap Expiry Cleanup

**Files:**
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Write test for expiry cleanup**

In `crates/server/src/state.rs`, add test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn expired_entries_are_cleaned_on_check() {
        let map: DashMap<String, RateLimitEntry> = DashMap::new();
        // Insert an expired entry (window started 20 minutes ago)
        map.insert(
            "old_ip".to_string(),
            RateLimitEntry {
                count: 5,
                window_start: chrono::Utc::now() - chrono::Duration::minutes(20),
            },
        );
        // Insert a fresh entry
        map.insert(
            "new_ip".to_string(),
            RateLimitEntry {
                count: 1,
                window_start: chrono::Utc::now(),
            },
        );

        // Trigger cleanup by checking a rate
        // After check, expired entry should be removed
        cleanup_expired_entries(&map, 15);
        assert!(!map.contains_key("old_ip"));
        assert!(map.contains_key("new_ip"));
    }
}
```

- [ ] **Step 2: Implement cleanup logic**

In `crates/server/src/state.rs`, add a cleanup function and call it from `check_rate`:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static RATE_CHECK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Remove entries older than `window_minutes` from the DashMap.
fn cleanup_expired_entries(map: &DashMap<String, RateLimitEntry>, window_minutes: i64) {
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(window_minutes);
    map.retain(|_, entry| entry.window_start > cutoff);
}
```

In the existing `check_rate` method (or `check_login_rate`/`check_register_rate`), add probabilistic cleanup at the start:

```rust
// Probabilistic cleanup: every 100 calls, sweep expired entries
let count = RATE_CHECK_COUNTER.fetch_add(1, Ordering::Relaxed);
if count % 100 == 0 {
    cleanup_expired_entries(&self.login_rate_limit, 15);
    cleanup_expired_entries(&self.register_rate_limit, 15);
}
```

Also, in the existing per-entry check, remove the current entry if it's expired (it already resets the window — just ensure it also removes if over window).

- [ ] **Step 3: Run tests**

Run: `cargo test -p serverbee-server state::tests`
Expected: test passes

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/state.rs
git commit -m "fix(security): add DashMap expiry cleanup for rate limiting

Probabilistic sweep every 100 checks removes entries older than
15 minutes, preventing unbounded memory growth."
```

---

## Task 6: libc Reentrant Functions

**Files:**
- Modify: `crates/agent/src/file_manager.rs`

- [ ] **Step 1: Replace getpwuid with getpwuid_r**

In `crates/agent/src/file_manager.rs`, replace `get_username_by_uid` (lines 628-637):

```rust
#[cfg(unix)]
fn get_username_by_uid(uid: u32) -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let mut passwd = unsafe { std::mem::zeroed::<libc::passwd>() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    loop {
        let ret = unsafe {
            libc::getpwuid_r(
                uid,
                &mut passwd,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                &mut result,
            )
        };

        if ret == libc::ERANGE {
            buf.resize(buf.len() * 2, 0);
            if buf.len() > 65536 {
                return Some(uid.to_string());
            }
            continue;
        }

        if ret != 0 || result.is_null() {
            return Some(uid.to_string());
        }

        let name = unsafe { std::ffi::CStr::from_ptr(passwd.pw_name) };
        return Some(name.to_string_lossy().to_string());
    }
}
```

- [ ] **Step 2: Replace getgrgid with getgrgid_r**

Replace `get_groupname_by_gid` (lines 641-651):

```rust
#[cfg(unix)]
fn get_groupname_by_gid(gid: u32) -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let mut group = unsafe { std::mem::zeroed::<libc::group>() };
    let mut result: *mut libc::group = std::ptr::null_mut();

    loop {
        let ret = unsafe {
            libc::getgrgid_r(
                gid,
                &mut group,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                &mut result,
            )
        };

        if ret == libc::ERANGE {
            buf.resize(buf.len() * 2, 0);
            if buf.len() > 65536 {
                return Some(gid.to_string());
            }
            continue;
        }

        if ret != 0 || result.is_null() {
            return Some(gid.to_string());
        }

        let name = unsafe { std::ffi::CStr::from_ptr(group.gr_name) };
        return Some(name.to_string_lossy().to_string());
    }
}
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p serverbee-agent`
Expected: compiles without errors (these are unix-only, so this must run on a Unix system)

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/file_manager.rs
git commit -m "fix(stability): use reentrant getpwuid_r/getgrgid_r

Non-reentrant getpwuid/getgrgid return pointers to static buffers,
causing data races in async/multi-thread contexts."
```

---

## Task 7: aggregate_hourly Migration

**Files:**
- Create: `crates/server/src/migration/m20260327_000012_records_hourly_unique.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create migration file**

Create `crates/server/src/migration/m20260327_000012_records_hourly_unique.rs`:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260327_000012_records_hourly_unique"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Step 1: Deduplicate existing data.
        // For each (server_id, hour_bucket), keep only the row with the largest id.
        db.execute_unprepared(
            "DELETE FROM records_hourly WHERE id NOT IN (
                SELECT MAX(id) FROM records_hourly
                GROUP BY server_id, strftime('%Y-%m-%d %H:00:00', time)
            )"
        ).await?;

        // Step 2: Align existing timestamps to hour boundaries.
        db.execute_unprepared(
            "UPDATE records_hourly SET time = strftime('%Y-%m-%d %H:00:00', time)"
        ).await?;

        // Step 3: Add unique index to prevent future duplicates.
        db.execute_unprepared(
            "CREATE UNIQUE INDEX idx_records_hourly_server_time ON records_hourly(server_id, time)"
        ).await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration in mod.rs**

In `crates/server/src/migration/mod.rs`:

Add the module declaration:
```rust
mod m20260327_000012_records_hourly_unique;
```

Add to the `migrations()` vec:
```rust
Box::new(m20260327_000012_records_hourly_unique::Migration),
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p serverbee-server`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(migration): add records_hourly dedup and unique index

Deduplicates existing hourly records by (server_id, hour_bucket),
aligns timestamps to hour boundaries, and adds UNIQUE(server_id, time)."
```

---

## Task 8: Rewrite aggregate_hourly

**Files:**
- Modify: `crates/server/src/service/record.rs`

- [ ] **Step 1: Rewrite aggregate_hourly with SQL upsert + disk_io_json hybrid**

In `crates/server/src/service/record.rs`, replace the `aggregate_hourly` function (lines ~200-283):

```rust
/// Aggregate records from the previous completed hour into hourly averages per server.
/// Uses SQL aggregation for numeric columns (pushed down to SQLite) and Rust-side
/// aggregation for disk_io_json (per-device grouping).
pub async fn aggregate_hourly(db: &DatabaseConnection) -> Result<u64, AppError> {
    let now = Utc::now();
    let hour = now
        .duration_trunc(chrono::Duration::hours(1))
        .map_err(|e| AppError::Internal(format!("Time truncation failed: {e}")))?;
    let hour_start = hour - chrono::Duration::hours(1);
    let hour_end = hour;

    let hour_start_str = hour_start.format("%Y-%m-%d %H:%M:%S").to_string();
    let hour_end_str = hour_end.format("%Y-%m-%d %H:%M:%S").to_string();

    // Step 1: SQL aggregation for numeric columns with upsert
    let sql = "INSERT INTO records_hourly \
        (server_id, time, cpu, mem_used, swap_used, disk_used, \
         net_in_speed, net_out_speed, net_in_transfer, net_out_transfer, \
         load1, load5, load15, tcp_conn, udp_conn, process_count, \
         temperature, gpu_usage) \
        SELECT \
            server_id, \
            ?, \
            AVG(cpu), \
            CAST(AVG(mem_used) AS INTEGER), \
            CAST(AVG(swap_used) AS INTEGER), \
            CAST(AVG(disk_used) AS INTEGER), \
            CAST(AVG(net_in_speed) AS INTEGER), \
            CAST(AVG(net_out_speed) AS INTEGER), \
            CAST(MAX(net_in_transfer) AS INTEGER), \
            CAST(MAX(net_out_transfer) AS INTEGER), \
            AVG(load1), \
            AVG(load5), \
            AVG(load15), \
            CAST(AVG(tcp_conn) AS INTEGER), \
            CAST(AVG(udp_conn) AS INTEGER), \
            CAST(AVG(process_count) AS INTEGER), \
            AVG(temperature), \
            AVG(gpu_usage) \
        FROM records \
        WHERE time >= ? AND time < ? \
        GROUP BY server_id \
        ON CONFLICT(server_id, time) DO UPDATE SET \
            cpu = excluded.cpu, \
            mem_used = excluded.mem_used, \
            swap_used = excluded.swap_used, \
            disk_used = excluded.disk_used, \
            net_in_speed = excluded.net_in_speed, \
            net_out_speed = excluded.net_out_speed, \
            net_in_transfer = excluded.net_in_transfer, \
            net_out_transfer = excluded.net_out_transfer, \
            load1 = excluded.load1, \
            load5 = excluded.load5, \
            load15 = excluded.load15, \
            tcp_conn = excluded.tcp_conn, \
            udp_conn = excluded.udp_conn, \
            process_count = excluded.process_count, \
            temperature = excluded.temperature, \
            gpu_usage = excluded.gpu_usage";

    let result = db
        .execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            sql,
            [
                hour_start_str.clone().into(),
                hour_start_str.clone().into(),
                hour_end_str.into(),
            ],
        ))
        .await?;

    let rows_affected = result.rows_affected();

    if rows_affected == 0 {
        return Ok(0);
    }

    // Step 2: disk_io_json aggregation (Rust-side, per server_id)
    // Fetch raw records for this hour to run aggregate_disk_io
    let records = record::Entity::find()
        .filter(record::Column::Time.gte(hour_start))
        .filter(record::Column::Time.lt(hour_end))
        .all(db)
        .await?;

    let mut grouped: HashMap<String, Vec<&record::Model>> = HashMap::new();
    for r in &records {
        grouped.entry(r.server_id.clone()).or_default().push(r);
    }

    for (server_id, server_records) in &grouped {
        let disk_io_json = aggregate_disk_io(server_records)?;
        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            "UPDATE records_hourly SET disk_io_json = ? WHERE server_id = ? AND time = ?",
            [
                disk_io_json.map(|s| s.into()).unwrap_or(sea_orm::Value::String(None)),
                server_id.clone().into(),
                hour_start_str.clone().into(),
            ],
        ))
        .await?;
    }

    Ok(rows_affected)
}
```

Note: You'll need `use sea_orm::Statement;` at the top if not already imported. Also add `use chrono::DurationRound;` for `duration_trunc`.

- [ ] **Step 2: Run existing tests**

Run: `cargo test -p serverbee-server service::record`
Expected: `test_aggregate_hourly_averages_disk_io_by_device` and other aggregate tests pass. If existing tests use `one_hour_ago` instead of truncated hour, they may need adjustment to insert records within the correct hour bucket.

- [ ] **Step 3: Add idempotency test**

Add a test that calls `aggregate_hourly` twice and verifies no duplicate records:

```rust
#[tokio::test]
async fn test_aggregate_hourly_idempotent() {
    let db = setup_test_db().await;
    // Insert test records in the previous hour bucket
    // ... (insert records with time in the previous hour)

    let count1 = RecordService::aggregate_hourly(&db).await.unwrap();
    let count2 = RecordService::aggregate_hourly(&db).await.unwrap();

    // Second call should upsert (update), not create new rows
    let all_hourly = record_hourly::Entity::find().all(&db).await.unwrap();
    // Should have exactly one row per server_id, not duplicates
    assert_eq!(all_hourly.len(), count1 as usize);
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test -p serverbee-server`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/record.rs
git commit -m "fix(stability): rewrite aggregate_hourly with hour-aligned upsert

- Truncate time to hour boundary (no more drifting timestamps)
- SQL aggregation for numeric columns (pushed to SQLite)
- ON CONFLICT upsert for idempotency
- Preserve Rust-side disk_io_json per-device aggregation"
```

---

## Task 9: ServerMessage::Upgrade sha256 + AgentConnection os/arch

**Files:**
- Modify: `crates/common/src/protocol.rs`
- Modify: `crates/server/src/service/agent_manager.rs`

- [ ] **Step 1: Add sha256 to ServerMessage::Upgrade**

In `crates/common/src/protocol.rs`, find the `Upgrade` variant and add `sha256`:

```rust
Upgrade {
    version: String,
    download_url: String,
    sha256: String,
},
```

- [ ] **Step 2: Add os/arch to AgentConnection**

In `crates/server/src/service/agent_manager.rs`, add fields to `AgentConnection`:

```rust
pub struct AgentConnection {
    pub server_id: String,
    pub server_name: String,
    pub tx: mpsc::Sender<ServerMessage>,
    pub connected_at: Instant,
    pub last_report_at: Instant,
    pub remote_addr: SocketAddr,
    pub protocol_version: u32,
    pub os: String,
    pub arch: String,
}
```

Initialize `os` and `arch` to empty strings in `add_connection`. Add a method to update them:

```rust
pub fn update_agent_info(&self, server_id: &str, os: String, arch: String) {
    if let Some(mut conn) = self.connections.get_mut(server_id) {
        conn.os = os;
        conn.arch = arch;
    }
}

pub fn get_agent_platform(&self, server_id: &str) -> Option<(String, String)> {
    self.connections.get(server_id).map(|c| (c.os.clone(), c.arch.clone()))
}
```

- [ ] **Step 3: Populate os/arch from SystemInfo**

In `crates/server/src/router/ws/agent.rs`, inside `handle_agent_message` where `AgentMessage::SystemInfo` is handled (around line 226), add:

```rust
AgentMessage::SystemInfo { msg_id, info } => {
    // ... existing GeoIP logic ...

    // Store os/arch for upgrade platform mapping
    state.agent_manager.update_agent_info(
        server_id,
        info.os.clone().unwrap_or_default(),
        info.cpu_arch.clone().unwrap_or_default(),
    );

    // ... rest of existing logic ...
}
```

Note: Check the actual field names in `SystemInfo` struct — they may be `os`, `cpu_arch`, or similar. Verify in `crates/common/src/protocol.rs` or the agent collector code.

- [ ] **Step 4: Fix compilation errors**

The `sha256` field addition will cause compile errors wherever `ServerMessage::Upgrade` is constructed. Fix `crates/server/src/router/api/server.rs` temporarily by adding a placeholder — this will be properly implemented in Task 11.

```rust
// Temporary — will be replaced in Task 11
ServerMessage::Upgrade {
    version: body.version,
    download_url: body.download_url,
    sha256: String::new(), // TODO: Task 11
}
```

- [ ] **Step 5: Verify build**

Run: `cargo build --workspace`
Expected: compiles without errors

- [ ] **Step 6: Commit**

```bash
git add crates/common/src/protocol.rs crates/server/src/service/agent_manager.rs crates/server/src/router/ws/agent.rs crates/server/src/router/api/server.rs
git commit -m "feat: add sha256 to Upgrade message, os/arch to AgentConnection

- ServerMessage::Upgrade now requires sha256 field
- AgentConnection stores os/arch from SystemInfo for platform mapping
- trigger_upgrade has temporary placeholder (next task)"
```

---

## Task 10: Agent Token Header Migration

**Files:**
- Modify: `crates/agent/src/reporter.rs`
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Server — accept token from Authorization header with query fallback**

In `crates/server/src/router/ws/agent.rs`:

1. Change `WsQuery` to `OptionalWsQuery`:
```rust
#[derive(Debug, Deserialize)]
pub struct OptionalWsQuery {
    token: Option<String>,
}
```

2. Add token extraction helper:
```rust
fn extract_agent_token(headers: &HeaderMap, query: &OptionalWsQuery) -> Option<String> {
    // Prefer Authorization header
    if let Some(auth) = headers.get("authorization") {
        if let Ok(val) = auth.to_str() {
            if let Some(token) = val.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }
    // Fallback to query param (deprecated)
    if let Some(ref token) = query.token {
        tracing::warn!("Agent using deprecated query param token — please upgrade agent");
        return Some(token.clone());
    }
    None
}
```

3. Update handler signature and token extraction:
```rust
pub async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<OptionalWsQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let token = extract_agent_token(&headers, &query)
        .ok_or_else(|| AppError::Unauthorized("Missing agent token".into()))?;

    // ... rest of validation using `token` instead of `query.token` ...
}
```

- [ ] **Step 2: Agent — send token via Authorization header**

In `crates/agent/src/reporter.rs`, modify `build_ws_url` and the connection logic:

```rust
fn build_ws_url(config: &AgentConfig) -> anyhow::Result<String> {
    let base = config.server_url.trim_end_matches('/');
    let ws_base = if base.starts_with("https://") {
        base.replacen("https://", "wss://", 1)
    } else if base.starts_with("http://") {
        base.replacen("http://", "ws://", 1)
    } else {
        format!("ws://{base}")
    };
    Ok(format!("{ws_base}/api/agent/ws"))  // Remove ?token=
}
```

Then, where `tokio_tungstenite::connect_async` is called, change to use a request with headers:

```rust
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;

let url = build_ws_url(config)?;
let uri: http::Uri = url.parse()?;
let host = uri.host().unwrap_or("localhost");

let request = Request::builder()
    .uri(&url)
    .header("Authorization", format!("Bearer {}", config.token))
    .header("Host", host)
    .header("Sec-WebSocket-Key", generate_key())
    .header("Sec-WebSocket-Version", "13")
    .header("Connection", "Upgrade")
    .header("Upgrade", "websocket")
    .body(())?;

let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
```

Note: Check if `connect_async` accepts a `Request` directly or if you need `connect_async_with_config`. The `tokio-tungstenite` 0.26 API should accept `impl IntoClientRequest`.

- [ ] **Step 3: Verify build**

Run: `cargo build --workspace`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/reporter.rs crates/server/src/router/ws/agent.rs
git commit -m "fix(security): move agent token from URL query to Authorization header

Token was visible in server/proxy logs via ?token= query param.
Now uses Authorization: Bearer header. Query param still accepted
with deprecation warning for backward compatibility."
```

---

## Task 11: Upgrade Mechanism — Server Side

**Files:**
- Modify: `crates/server/src/router/api/server.rs`

- [ ] **Step 1: Add platform mapping helper**

In `crates/server/src/router/api/server.rs`, add:

```rust
/// Map agent-reported OS string to release asset platform suffix.
fn map_os(os: &str) -> Option<&'static str> {
    let lower = os.to_lowercase();
    if lower.contains("linux") {
        Some("linux")
    } else if lower.contains("mac") || lower.contains("darwin") {
        Some("darwin")
    } else if lower.contains("windows") {
        Some("windows")
    } else {
        None
    }
}

/// Map Rust arch string to release asset arch suffix.
fn map_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// Normalize version string: strip optional 'v' prefix.
fn normalize_version(version: &str) -> &str {
    version.strip_prefix('v').unwrap_or(version)
}
```

- [ ] **Step 2: Rewrite trigger_upgrade handler**

Replace the `trigger_upgrade` handler:

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpgradeRequest {
    /// Target version string (e.g. "0.2.0" or "v0.2.0")
    version: String,
}

pub async fn trigger_upgrade(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpgradeRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    let server = ServerService::get_server(&state.db, &id).await?;
    let caps = server.capabilities as u32;
    if !has_capability(caps, CAP_UPGRADE) {
        return Err(AppError::Forbidden(
            "Upgrade capability not enabled for this server".into(),
        ));
    }

    let version = normalize_version(&body.version);

    // Validate version format (basic semver: digits and dots)
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(AppError::BadRequest("Invalid version format".into()));
    }

    // Get agent platform info
    let (os_raw, arch_raw) = state
        .agent_manager
        .get_agent_platform(&id)
        .ok_or_else(|| AppError::NotFound("Agent not connected or platform info unavailable".into()))?;

    let os = map_os(&os_raw)
        .ok_or_else(|| AppError::BadRequest(format!("Unsupported agent OS: {os_raw}")))?;
    let arch = map_arch(&arch_raw)
        .ok_or_else(|| AppError::BadRequest(format!("Unsupported agent arch: {arch_raw}")))?;

    // Build asset name
    let asset_name = if os == "windows" {
        format!("serverbee-agent-{os}-{arch}.exe")
    } else {
        format!("serverbee-agent-{os}-{arch}")
    };

    let base_url = &state.config.upgrade.release_base_url;
    let download_url = format!("{base_url}/download/v{version}/{asset_name}");

    // Fetch checksums.txt from release
    let checksums_url = format!("{base_url}/download/v{version}/checksums.txt");
    let checksums_body = reqwest::get(&checksums_url)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch checksums: {e}")))?
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read checksums: {e}")))?;

    // Parse checksums.txt: each line is "<sha256>  <filename>"
    let sha256 = checksums_body
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(2, |c: char| c.is_whitespace());
            let hash = parts.next()?;
            let name = parts.next()?.trim();
            if name == asset_name {
                Some(hash.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Checksum not found for {asset_name} in v{version} release"
            ))
        })?;

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or_else(|| AppError::NotFound("Agent not connected".into()))?;

    let msg = ServerMessage::Upgrade {
        version: version.to_string(),
        download_url,
        sha256,
    };
    sender
        .send(msg)
        .await
        .map_err(|_| AppError::Internal("Failed to send upgrade command".into()))?;

    ok("ok")
}
```

Note: `reqwest` should already be available as a dependency since the agent uses it. If not in the server crate, add `reqwest = { version = "0.12", features = ["rustls-tls"] }` to `crates/server/Cargo.toml`.

- [ ] **Step 3: Add tests for platform mapping**

```rust
#[cfg(test)]
mod upgrade_tests {
    use super::*;

    #[test]
    fn test_map_os() {
        assert_eq!(map_os("Linux 5.15.0-123-generic"), Some("linux"));
        assert_eq!(map_os("macOS 14.1.2 23B92 arm64"), Some("darwin"));
        assert_eq!(map_os("Mac OS X 13.0"), Some("darwin"));
        assert_eq!(map_os("Windows 10 Pro 22H2"), Some("windows"));
        assert_eq!(map_os("FreeBSD 13.2"), None);
    }

    #[test]
    fn test_map_arch() {
        assert_eq!(map_arch("x86_64"), Some("amd64"));
        assert_eq!(map_arch("aarch64"), Some("arm64"));
        assert_eq!(map_arch("arm"), None);
    }

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("v0.7.1"), "0.7.1");
        assert_eq!(normalize_version("0.7.1"), "0.7.1");
        assert_eq!(normalize_version("v1.0.0"), "1.0.0");
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-server upgrade_tests`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/server.rs
git commit -m "fix(security): version-driven upgrade with server-side URL resolution

Admin now only submits version string. Server resolves download URL
and sha256 from configured release source. Prevents SSRF and
malicious binary injection via arbitrary download_url."
```

---

## Task 12: Upgrade Mechanism — Agent Side

**Files:**
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Rewrite perform_upgrade with mandatory sha256**

In `crates/agent/src/reporter.rs`, replace `perform_upgrade`:

```rust
async fn perform_upgrade(version: &str, download_url: &str, sha256: &str) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    // Validate URL scheme
    if !download_url.starts_with("https://") {
        anyhow::bail!("Upgrade URL must use HTTPS, got: {download_url}");
    }

    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("bak");

    tracing::info!("Downloading agent v{version} from {download_url}...");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10 minute timeout
        .build()?;
    let response = client
        .get(download_url)
        .header("User-Agent", "ServerBee-Agent")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status {}", response.status());
    }

    let bytes = response.bytes().await?;
    tracing::info!("Downloaded {} bytes", bytes.len());

    // Mandatory SHA-256 verification
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = format!("{:x}", hasher.finalize());
    if actual != sha256 {
        anyhow::bail!("Checksum mismatch: expected {sha256}, got {actual}");
    }
    tracing::info!("Checksum verified");

    // Write to temporary file
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Backup current binary and replace
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }
    std::fs::rename(&current_exe, &backup_path)?;
    std::fs::rename(&tmp_path, &current_exe)?;

    tracing::info!("Agent binary replaced. Restarting...");

    // Restart: exec the new binary with the same args
    let args: Vec<String> = std::env::args().collect();
    let mut cmd = std::process::Command::new(&current_exe);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    cmd.spawn()?;

    std::process::exit(0);
}
```

- [ ] **Step 2: Update the Upgrade message handler**

Find where `ServerMessage::Upgrade` is matched (around line 527) and update to pass `sha256`:

```rust
ServerMessage::Upgrade { version, download_url, sha256 } => {
    let caps = capabilities.load(Ordering::SeqCst);
    if !has_capability(caps, CAP_UPGRADE) {
        // ... existing capability denial ...
        return;
    }
    tokio::spawn(async move {
        if let Err(e) = perform_upgrade(&version, &download_url, &sha256).await {
            tracing::error!("Upgrade failed: {e}");
        }
    });
}
```

- [ ] **Step 3: Verify build**

Run: `cargo build --workspace`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "fix(security): mandatory sha256 verification in agent upgrade

- HTTPS-only download URL enforcement
- SHA-256 checksum is mandatory (no more optional header check)
- 10-minute download timeout
- Removes x-checksum-sha256 header fallback"
```

---

## Task 13: Frontend WebSocket Robustness

**Files:**
- Modify: `apps/web/src/hooks/use-terminal-ws.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts`

- [ ] **Step 1: Fix use-terminal-ws.ts**

Replace the `ws.onmessage` handler (lines 34-53):

```typescript
ws.onmessage = (event) => {
    let msg: TerminalMessage
    try {
        msg = JSON.parse(event.data as string)
    } catch {
        console.warn('Terminal WS: invalid JSON', event.data)
        return
    }
    switch (msg.type) {
        case 'output':
            if (typeof msg.data === 'string' && onDataRef.current) {
                try {
                    const decoded = atob(msg.data)
                    onDataRef.current(decoded)
                } catch {
                    console.warn('Terminal WS: invalid base64 data')
                }
            }
            break
        case 'started':
            break
        case 'error':
            setError(msg.error ?? 'Unknown error')
            break
        case 'session':
            break
        default:
            break
    }
}
```

- [ ] **Step 2: Fix use-servers-ws.ts**

Add `isWsMessageLike` guard function before `useServersWs`:

```typescript
function isWsMessageLike(raw: unknown): raw is { type: string } & Record<string, unknown> {
    return (
        typeof raw === 'object' &&
        raw !== null &&
        'type' in raw &&
        typeof (raw as { type: unknown }).type === 'string'
    )
}
```

Replace the `ws.onMessage` callback (lines 154-242):

```typescript
ws.onMessage((raw) => {
    if (!isWsMessageLike(raw)) {
        console.warn('WS: unexpected message shape', raw)
        return
    }

    switch (raw.type) {
        case 'full_sync':
        case 'update': {
            if (!Array.isArray(raw.servers) || raw.servers.some((s: unknown) => s == null || typeof s !== 'object')) break
            const msg = raw as WsMessage & { type: 'full_sync' | 'update' }
            if (raw.type === 'full_sync') {
                queryClient.setQueryData<ServerMetrics[]>(['servers'], msg.servers)
            } else {
                queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
                    if (!prev) {
                        return msg.servers
                    }
                    return mergeServerUpdate(prev, msg.servers)
                })
            }
            break
        }
        case 'server_online': {
            if (typeof raw.server_id !== 'string') break
            const msg = raw as WsMessage & { type: 'server_online' }
            queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
                if (!prev) {
                    return prev
                }
                return setServerOnlineStatus(prev, msg.server_id, true)
            })
            break
        }
        case 'server_offline': {
            if (typeof raw.server_id !== 'string') break
            const msg = raw as WsMessage & { type: 'server_offline' }
            queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
                if (!prev) {
                    return prev
                }
                return setServerOnlineStatus(prev, msg.server_id, false)
            })
            break
        }
        case 'capabilities_changed': {
            if (typeof raw.server_id !== 'string' || typeof raw.capabilities !== 'number') break
            const msg = raw as WsMessage & { type: 'capabilities_changed' }
            const { server_id, capabilities } = msg
            queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
                prev?.map((s) => (s.id === server_id ? { ...s, capabilities } : s))
            )
            queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
                prev ? { ...prev, capabilities } : prev
            )
            queryClient.invalidateQueries({ queryKey: ['servers-list'] })
            break
        }
        case 'agent_info_updated': {
            if (typeof raw.server_id !== 'string' || typeof raw.protocol_version !== 'number') break
            const msg = raw as WsMessage & { type: 'agent_info_updated' }
            const { server_id, protocol_version } = msg
            queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
                prev ? { ...prev, protocol_version } : prev
            )
            queryClient.invalidateQueries({ queryKey: ['servers-list'] })
            break
        }
        case 'network_probe_update': {
            if (typeof raw.server_id !== 'string' || !Array.isArray(raw.results) || raw.results.some((r: unknown) => r == null || typeof r !== 'object')) break
            const msg = raw as WsMessage & { type: 'network_probe_update' }
            window.dispatchEvent(
                new CustomEvent('network-probe-update', {
                    detail: { server_id: msg.server_id, results: msg.results }
                })
            )
            break
        }
        case 'docker_update': {
            if (typeof raw.server_id !== 'string' || !Array.isArray(raw.containers) || raw.containers.some((c: unknown) => c == null || typeof c !== 'object')) break
            const msg = raw as WsMessage & { type: 'docker_update' }
            const { server_id, containers, stats } = msg
            queryClient.setQueryData<DockerContainer[]>(['docker', 'containers', server_id], containers)
            if (stats) {
                queryClient.setQueryData<DockerContainerStats[]>(['docker', 'stats', server_id], stats)
            }
            break
        }
        case 'docker_event': {
            if (typeof raw.server_id !== 'string' || typeof raw.event !== 'object' || raw.event === null) break
            const msg = raw as WsMessage & { type: 'docker_event' }
            const { server_id, event } = msg
            queryClient.setQueryData<DockerEventInfo[]>(['docker', 'events', server_id], (prev) => {
                const events = prev ?? []
                const updated = [event, ...events]
                return updated.length > MAX_DOCKER_EVENTS ? updated.slice(0, MAX_DOCKER_EVENTS) : updated
            })
            break
        }
        case 'docker_availability_changed': {
            if (typeof raw.server_id !== 'string' || typeof raw.available !== 'boolean') break
            const msg = raw as WsMessage & { type: 'docker_availability_changed' }
            const { server_id, available } = msg
            queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
                prev ? setServerDockerAvailability(prev, server_id, available) : prev
            )
            queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
                setServerDetailDockerAvailability(prev, available)
            )
            break
        }
        default:
            break
    }
})
```

- [ ] **Step 3: Run frontend checks**

Run:
```bash
cd apps/web && bun run typecheck && bun x ultracite check && bun run test
```
Expected: all pass

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/hooks/use-terminal-ws.ts apps/web/src/hooks/use-servers-ws.ts
git commit -m "fix(stability): add runtime validation to WebSocket message handlers

- Terminal WS: try/catch on JSON.parse and atob, typeof guard on data
- Main WS: two-layer validation (isWsMessageLike + per-variant field checks)
- Prevents uncaught exceptions from malformed server messages"
```

---

## Task 14: Release Pipeline Checksums

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Add checksum generation step**

In `.github/workflows/release.yml`, in the `release` job, add a step between "Download all artifacts" (line 222) and "Create GitHub Release" (line 224):

```yaml
      - name: Generate checksums
        run: |
          cd artifacts
          sha256sum * > checksums.txt
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: generate checksums.txt in release pipeline

Required by the version-driven upgrade mechanism. Server downloads
checksums.txt from the release to verify agent binary integrity."
```

---

## Task 15: Documentation & OpenAPI Regeneration

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`
- Regenerate: `apps/web/openapi.json`, `apps/web/src/lib/api-types.ts`

- [ ] **Step 1: Regenerate OpenAPI types**

Run:
```bash
cd apps/web && bun run generate:api-types
```

Verify that `openapi.json` no longer contains `download_url` in `UpgradeRequest`:
```bash
grep -c download_url apps/web/openapi.json
# Expected: 0 (or only in non-UpgradeRequest contexts)
```

- [ ] **Step 2: Update ENV.md**

Add new environment variables to `ENV.md`:

```markdown
| `SERVERBEE_SERVER__TRUSTED_PROXIES` | `[]` | CIDR list of trusted reverse proxies (e.g. `["127.0.0.1/32", "10.0.0.0/8"]`) |
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `https://github.com/ZingerLittleBee/ServerBee/releases` | Base URL for agent upgrade releases |
```

- [ ] **Step 3: Update docs configuration pages**

Add the same config documentation to both `apps/docs/content/docs/en/configuration.mdx` and `apps/docs/content/docs/cn/configuration.mdx`.

- [ ] **Step 4: Final verification**

Run all checks:
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cd apps/web && bun run typecheck && bun x ultracite check && bun run test
```

Expected: all pass with 0 warnings

- [ ] **Step 5: Commit**

```bash
git add ENV.md apps/docs/ apps/web/openapi.json apps/web/src/lib/api-types.ts
git commit -m "docs: update config docs and regenerate OpenAPI types

- Add trusted_proxies and release_base_url to ENV.md and config docs
- Regenerate openapi.json and api-types.ts (UpgradeRequest simplified)"
```

---

## Verification Checklist

After all tasks complete, run the full verification:

```bash
# Rust
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings

# Frontend
cd apps/web
bun run typecheck
bun x ultracite check
bun run test

# OpenAPI sync
bun run generate:api-types
# Confirm UpgradeRequest no longer has download_url
```

**Targeted regression tests:**
- `aggregate_hourly`: idempotency (two calls, no duplicates), hour-aligned timestamps
- WS guards: all 10 message types pass, malformed payloads silently dropped
- Terminal WS: invalid JSON, non-string data, invalid base64 all handled
- Rate limiting: spoofed XFF ignored when source not in trusted_proxies
- Agent token: Bearer header works, query param fallback with warning
