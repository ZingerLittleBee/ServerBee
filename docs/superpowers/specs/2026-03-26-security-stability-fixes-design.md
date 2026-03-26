# 安全与稳定性修复设计文档

**日期**: 2026-03-26
**来源**: `docs/code-review-2026-03-26.md` 代码审查报告
**范围**: 第一批（安全关键 4 项）+ 第二批（稳定性 4 项），共 8 项修复

---

## 修复清单

| # | 分类 | 问题 | 涉及 Crate |
|---|------|------|-----------|
| 1 | 安全 | 速率限制可被 X-Forwarded-For 欺骗绕过 | server |
| 2 | 安全 | CORS 全开放策略 | server |
| 3 | 安全 | WebSocket 消息大小未实际强制执行 | server |
| 4 | 安全 | Upgrade 机制：download_url 无校验 + 校验和可选 | server, agent, common |
| 5 | 稳定性 | Agent getpwuid/getgrgid 非重入版本在 async 环境下存在 UB | agent |
| 6 | 稳定性 | aggregate_hourly 无重复插入保护 | server |
| 7 | 稳定性 | 前端 WebSocket 消息处理缺少健壮性 | web |
| 8 | 稳定性 | Agent Token 明文暴露在 WebSocket URL 中 | server, agent |

---

## 1. 速率限制修复

### 问题

`extract_client_ip` 无条件信任 `X-Forwarded-For` header，攻击者可在每次登录尝试时携带不同伪造 IP，完全绕过 15 分钟速率限制。此函数在 4 个文件中重复定义（`auth.rs`、`file.rs`、`server.rs`、`agent.rs`），实现略有差异。DashMap 中的过期条目从不主动清理，长期运行内存持续增长。

**相关文件**:
- `crates/server/src/router/api/auth.rs:594-605`
- `crates/server/src/router/api/file.rs:131-143`
- `crates/server/src/router/api/oauth.rs:186-191`（内联实现，还缺少 `x-real-ip` 回退）
- `crates/server/src/state.rs:55-100`

### 方案

#### 1.1 配置化信任代理

在 `AppConfig` 中新增配置：

```toml
[server]
trusted_proxies = ["127.0.0.1/32", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"]
```

环境变量：`SERVERBEE_SERVER__TRUSTED_PROXIES`

默认值为空数组（不信任任何代理，直接用 TCP 源 IP）。

新增 `ipnet` crate 依赖做 CIDR 匹配。

#### 1.2 统一 extract_client_ip

从 5 个文件中提取到 `crates/server/src/router/utils.rs` 共享模块（`auth.rs`、`file.rs`、`server.rs`、`agent.rs`、`oauth.rs`）：

```rust
pub fn extract_client_ip(
    connect_info: &ConnectInfo<SocketAddr>,
    headers: &HeaderMap,
    trusted_proxies: &[IpNet],
) -> IpAddr
```

判断规则：
1. 获取 TCP 连接源 IP：`connect_info.0.ip()`
2. 若源 IP 匹配 `trusted_proxies` 中任一 CIDR → 解析 `X-Forwarded-For`，从右向左找第一个不在 trusted_proxies 中的 IP
3. 否则 → 直接返回 TCP 源 IP
4. 所有无法解析的情况回退到 TCP 源 IP

原 5 个文件中的 `extract_client_ip`（含 `oauth.rs:186` 的内联实现）全部替换为调用此共享函数。

#### 1.3 DashMap 过期条目清理

在 `state.rs` 的 `check_rate()` 方法中增加惰性清理：每次调用时检查当前条目是否过期，过期则移除。额外在每 100 次调用时触发一次全量扫描（概率清理），移除所有超过 15 分钟窗口的条目。

#### 1.4 ConnectInfo 支持

在 Router 层确保 `ConnectInfo<SocketAddr>` 可用。Axum 的 `TcpListener` 默认支持，需在需要 IP 的 handler 中添加 `ConnectInfo<SocketAddr>` extractor。

---

## 2. 移除 CORS

### 问题

`CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)` 暴露全部 API 给任意域。

**相关文件**: `crates/server/src/router/mod.rs:17-20`

### 方案

直接删除 `router/mod.rs` 中的 `CorsLayer` 及相关 import。

SPA 通过 rust-embed 嵌入 server 二进制，前端和 API 始终同源，不存在跨域场景。API Key 调用来自脚本/Agent，不受浏览器 CORS 限制。

改动量：约 5 行删除。

---

## 3. WebSocket 消息大小限制

### 问题

`MAX_WS_MESSAGE_SIZE`（1MB）在 `constants.rs` 定义但从未被实际配置到 WebSocket 处理器中。

**相关文件**:
- `crates/common/src/constants.rs:10`
- `crates/server/src/router/ws/agent.rs`
- `crates/server/src/router/ws/browser.rs`
- `crates/server/src/router/ws/terminal.rs`
- `crates/server/src/router/ws/docker_logs.rs`

### 方案

在所有 WebSocket handler 中，`WebSocketUpgrade` 的 `on_upgrade()` 调用前配置消息大小限制：

```rust
ws.max_message_size(MAX_WS_MESSAGE_SIZE)
  .on_upgrade(move |socket| handle_socket(socket, ...))
```

涉及 4 个 WebSocket handler 文件，每个文件改动约 1 行。

注意：Axum 0.8 的 `WebSocketUpgrade` 通过 `.max_message_size(usize)` 方法配置，需确认 API 是否为 `max_message_size` 或 `max_frame_size`，以实际 axum 0.8 版本 API 为准。

---

## 4. Upgrade 机制重构（版本驱动）

### 问题

`trigger_upgrade` 端点的 `download_url` 完全由 admin 提交，未经校验，直接透传给 Agent 执行下载。Agent 侧校验和仅在响应头 `x-checksum-sha256` 存在时才验证，否则直接替换二进制。

**相关文件**:
- `crates/server/src/router/api/server.rs:444-471`
- `crates/agent/src/reporter.rs:1579-1653`
- `crates/common/src/` (ServerMessage 定义)

### 方案

Admin 只提交 `version`，Server 负责从可信发布源解析 `download_url` + `sha256`，Agent 负责下载并强制校验。

#### 4.1 配置

```toml
[upgrade]
release_base_url = "https://github.com/ZingerLittleBee/ServerBee/releases"
```

环境变量：`SERVERBEE_UPGRADE__RELEASE_BASE_URL`

默认值为 GitHub 官方仓库地址。用户可配置自建 mirror。

#### 4.2 Server 端改动

**UpgradeRequest** 简化：

```rust
pub struct UpgradeRequest {
    pub version: String,  // 移除 download_url
}
```

**trigger_upgrade handler 流程**：

1. **版本规范化**：校验 `version` 格式（semver），允许可选 `v` 前缀，处理时先 strip 前缀得到纯数字版本号 `normalized_version`（例如 `v0.7.1` → `0.7.1`，`0.7.1` → `0.7.1`），后续一律使用 `normalized_version`
2. 从 `AgentManager` 获取目标 Agent 的 `os` + `arch`（Agent 在 WS 握手后的首条 `SystemInfo` 消息中上报）
3. **平台映射**：Agent 上报的 `os` 是 `System::long_os_version()` 长字符串（如 `"Linux 5.15.0-123-generic"`），`arch` 是 `std::env::consts::ARCH`（如 `"x86_64"`）。需要映射为 release 产物的命名后缀：
   - OS 映射：包含 `"Linux"` → `linux`，包含 `"Mac"` 或 `"macOS"` → `darwin`，包含 `"Windows"` → `windows`
   - Arch 映射：`x86_64` → `amd64`，`aarch64` → `arm64`
   - 映射失败则返回错误，拒绝触发升级
4. 拼接 asset 名称：`serverbee-agent-{mapped_os}-{mapped_arch}`，Windows 加 `.exe`
5. 拼接下载 URL：`{release_base_url}/download/v{normalized_version}/{asset_name}`
6. 获取 sha256：
   - 下载 `{release_base_url}/download/v{normalized_version}/checksums.txt`
   - 文件格式为标准 `sha256sum` 输出：每行 `<hex_sha256>  <filename>`
   - 解析内容找到 `{asset_name}` 对应的 sha256 值
   - 若 checksums.txt 下载失败或找不到对应 asset 条目，返回错误，拒绝触发升级
7. 发送 `ServerMessage::Upgrade { version, download_url, sha256 }` 给 Agent

**AgentConnection 扩展**：

Agent 连接时的 `SystemInfo` 消息中已包含 `os`（`System::long_os_version()` 长字符串）和 `arch`（`std::env::consts::ARCH`）信息。在 `agent_manager.rs` 的 `AgentConnection` 结构体中新增 `os: String` 和 `arch: String` 字段，在收到首条 `SystemInfo` 消息时填充，供 upgrade 时做平台映射。

#### 4.6 Release Pipeline 改动

当前 `.github/workflows/release.yml` 不生成 checksums.txt。需要在 `release` job 的 "Download all artifacts" 步骤之后、"Create GitHub Release" 步骤之前，新增一步：

```yaml
- name: Generate checksums
  run: |
    cd artifacts
    sha256sum * > checksums.txt
```

将 `checksums.txt` 与其他产物一起上传到 GitHub Release。

#### 4.3 Common crate 改动

`ServerMessage::Upgrade` 增加必填字段：

```rust
ServerMessage::Upgrade {
    version: String,
    download_url: String,
    sha256: String,          // 新增，必填
}
```

#### 4.4 Agent 端改动

`perform_upgrade` 流程改为：

1. 校验 `download_url` scheme 必须为 `https://`
2. 下载二进制到临时文件（添加超时，建议 10 分钟）
3. 计算下载文件的 SHA-256
4. 与消息中的 `sha256` 字段**强制比对**
5. 不匹配 → 删除临时文件，上报错误，中止升级
6. 匹配 → 替换当前二进制并重启

移除原来的 `x-checksum-sha256` 响应头可选校验逻辑。

#### 4.7 前端改动

当前代码库中不存在 Upgrade 触发 UI 表单。`UpgradeRequest` schema 变更后，需要：
- 重新生成 `openapi.json` 和 `api-types.ts`（通过 `bun run generate:api-types`）
- 若后续实现 Upgrade UI，表单仅需 `version` 字段

---

## 5. libc 非重入函数修复

### 问题

`libc::getpwuid` / `libc::getgrgid` 返回指向静态缓冲区的指针，在多线程 async 环境下存在数据竞争（UB）。

**相关文件**: `crates/agent/src/file_manager.rs:628-650`

### 方案

替换为 POSIX 重入版本 `getpwuid_r` / `getgrgid_r`：

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
            // 缓冲区不够，扩容后重试
            buf.resize(buf.len() * 2, 0);
            if buf.len() > 65536 { return Some(uid.to_string()); }
            continue;
        }

        if ret != 0 || result.is_null() {
            return Some(uid.to_string()); // 查找失败，回退到数字
        }

        let name = unsafe { std::ffi::CStr::from_ptr(passwd.pw_name) };
        return Some(name.to_string_lossy().to_string());
    }
}
```

`get_groupname_by_gid` 同理，使用 `getgrgid_r` + `libc::group` 结构体。

初始缓冲区 1024 字节，ERANGE 时扩容到最大 64KB，超过则回退到数字 ID。

---

## 6. aggregate_hourly 幂等修复

### 问题

`aggregate_hourly` 的时间戳是 `now - 1h` 而非整点小时桶，调度器每 3600 秒执行而非时钟对齐。同一小时可能写出不同时间戳的多条记录，即使加唯一约束也无法防止。

**相关文件**:
- `crates/server/src/service/record.rs:200-307`
- `crates/server/src/task/aggregator.rs:10-12`

**参考**: `crates/server/src/service/traffic.rs:26`（upsert 模式）、`crates/server/src/service/network_probe.rs:1011`（小时桶 + upsert）

### 方案

#### 6.1 时间对齐

将聚合时间截断到整点小时桶：

```rust
let now = Utc::now();
let hour = now
    .duration_trunc(chrono::Duration::hours(1))
    .unwrap();
let hour_start = hour - chrono::Duration::hours(1); // 上一个完整小时
let hour_end = hour; // 当前小时整点
```

查询范围改为 `[hour_start, hour_end)` 的原始记录。

#### 6.2 改用 SQL 聚合 + upsert

参考 `network_probe.rs:1011-1020` 模式，改用 raw SQL。

实际表名为 `records_hourly`（注意 `records` 复数），源表为 `records`。列名与 entity 定义一致（`cpu`、`mem_used`、`swap_used`、`disk_used` 等），不是 avg/max 拆分列。

**采用混合策略**：数值列用 SQL 聚合下推到 SQLite，`disk_io_json` 保留 Rust 侧聚合（当前 `aggregate_disk_io` 函数已有按设备名分组取平均的逻辑，且有测试覆盖）。

**注意整数列类型**：`mem_used`、`swap_used`、`disk_used`、`net_in_speed`、`net_out_speed`、`net_in_transfer`、`net_out_transfer` 为 i64，`tcp_conn`、`udp_conn`、`process_count` 为 i32。SQLite 的 `AVG()` 返回 REAL，需显式 `CAST(... AS INTEGER)` 避免写入浮点值导致 ORM 读取不稳定。

数值列 SQL（不含 disk_io_json）：

```sql
INSERT INTO records_hourly
    (server_id, time, cpu, mem_used, swap_used, disk_used,
     net_in_speed, net_out_speed, net_in_transfer, net_out_transfer,
     load1, load5, load15, tcp_conn, udp_conn, process_count,
     temperature, gpu_usage)
SELECT
    server_id,
    ?,  -- hour_start (整点)
    AVG(cpu),                              -- f64, 无需 CAST
    CAST(AVG(mem_used) AS INTEGER),        -- i64
    CAST(AVG(swap_used) AS INTEGER),       -- i64
    CAST(AVG(disk_used) AS INTEGER),       -- i64
    CAST(AVG(net_in_speed) AS INTEGER),    -- i64
    CAST(AVG(net_out_speed) AS INTEGER),   -- i64
    CAST(MAX(net_in_transfer) AS INTEGER), -- i64, 累计值取最大
    CAST(MAX(net_out_transfer) AS INTEGER),-- i64
    AVG(load1),                            -- f64
    AVG(load5),                            -- f64
    AVG(load15),                           -- f64
    CAST(AVG(tcp_conn) AS INTEGER),        -- i32
    CAST(AVG(udp_conn) AS INTEGER),        -- i32
    CAST(AVG(process_count) AS INTEGER),   -- i32
    AVG(temperature),                      -- Option<f64>
    AVG(gpu_usage)                         -- Option<f64>
FROM records
WHERE time >= ? AND time < ?
GROUP BY server_id
ON CONFLICT(server_id, time) DO UPDATE SET
    cpu = excluded.cpu,
    mem_used = excluded.mem_used,
    swap_used = excluded.swap_used,
    disk_used = excluded.disk_used,
    net_in_speed = excluded.net_in_speed,
    net_out_speed = excluded.net_out_speed,
    net_in_transfer = excluded.net_in_transfer,
    net_out_transfer = excluded.net_out_transfer,
    load1 = excluded.load1,
    load5 = excluded.load5,
    load15 = excluded.load15,
    tcp_conn = excluded.tcp_conn,
    udp_conn = excluded.udp_conn,
    process_count = excluded.process_count,
    temperature = excluded.temperature,
    gpu_usage = excluded.gpu_usage
```

**disk_io_json 单独处理**：SQL 插入/更新后，对每个 server_id 执行 Rust 侧 `aggregate_disk_io`（复用现有函数），然后单独 `UPDATE records_hourly SET disk_io_json = ? WHERE server_id = ? AND time = ?`。这样保持了按设备名分组聚合的能力，不回归现有测试。

#### 6.3 新增 migration

1. **去重现有数据**：对每个 `(server_id, strftime('%Y-%m-%d %H:00:00', time))` 组，保留 `id` 最大的一条，删除其余
2. **时间戳对齐**：`UPDATE records_hourly SET time = strftime('%Y-%m-%d %H:00:00', time)`
3. **添加唯一索引**：`CREATE UNIQUE INDEX idx_records_hourly_server_time ON records_hourly(server_id, time)`

---

## 7. 前端 WebSocket 错误处理

### 问题

- `use-terminal-ws.ts:35`：`JSON.parse(event.data)` 无 try/catch
- `use-servers-ws.ts:154`：`const msg = raw as WsMessage` 无运行时校验

**相关文件**:
- `apps/web/src/hooks/use-terminal-ws.ts:35`
- `apps/web/src/hooks/use-servers-ws.ts:154`

### 方案

#### 7.1 use-terminal-ws.ts

`JSON.parse` 包裹 try/catch，`output` 分支中对 `msg.data` 增加类型校验和 `atob` 保护：

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
        // ... 其余 case 不变
    }
}
```

保护要点：
1. `JSON.parse` try/catch — 拦截非法 JSON
2. `typeof msg.data === 'string'` — 拦截缺失或非字符串 data
3. `atob` try/catch — 拦截非法 base64 编码

#### 7.2 use-servers-ws.ts

替换 `as WsMessage` 断言，改为两层运行时校验：顶层 guard 检查基本结构，switch 内每个 case 检查该 variant 的必需字段。

**顶层 guard**（检查是否为含 `type` 字符串字段的对象）：

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

**switch 内每个 case 加必需字段检查**，畸形对象在此被拦截，不再盲目解引用：

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
            // ... 原有逻辑
            break
        }
        case 'server_online':
        case 'server_offline': {
            if (typeof raw.server_id !== 'string') break
            const msg = raw as WsMessage & { type: 'server_online' | 'server_offline' }
            // ...
            break
        }
        case 'capabilities_changed': {
            if (typeof raw.server_id !== 'string' || typeof raw.capabilities !== 'number') break
            // ...
            break
        }
        case 'agent_info_updated': {
            if (typeof raw.server_id !== 'string' || typeof raw.protocol_version !== 'number') break
            // ...
            break
        }
        case 'network_probe_update': {
            if (typeof raw.server_id !== 'string' || !Array.isArray(raw.results) || raw.results.some((r: unknown) => r == null || typeof r !== 'object')) break
            // ...
            break
        }
        case 'docker_update': {
            if (typeof raw.server_id !== 'string' || !Array.isArray(raw.containers) || raw.containers.some((c: unknown) => c == null || typeof c !== 'object')) break
            // ...
            break
        }
        case 'docker_event': {
            if (typeof raw.server_id !== 'string' || typeof raw.event !== 'object' || raw.event === null) break
            // ...
            break
        }
        case 'docker_availability_changed': {
            if (typeof raw.server_id !== 'string' || typeof raw.available !== 'boolean') break
            // ...
            break
        }
        default:
            console.warn('WS: unknown message type', raw.type)
    }
})
```

不引入 Zod，保持零新依赖。若 Rust 端 `BrowserMessage` 新增 variant（如 `ServerIpChanged`），需同步更新 `WsMessage` 类型定义和对应 case。

---

## 8. Agent Token 传输方式迁移

### 问题

Token 在 WebSocket URL `?token=xxx` 中，会被服务器/代理/CDN 日志记录。

**相关文件**:
- `crates/agent/src/reporter.rs:1325`
- `crates/server/src/router/ws/agent.rs:26-31`

### 方案

#### 8.1 Agent 端（reporter.rs）

使用 `tungstenite::http::Request::builder()` 构造带 header 的 WebSocket 连接请求：

```rust
let url = format!("{ws_base}/api/agent/ws"); // 移除 ?token=
let request = tungstenite::http::Request::builder()
    .uri(&url)
    .header("Authorization", format!("Bearer {}", config.token))
    .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key())
    .header("Sec-WebSocket-Version", "13")
    .header("Connection", "Upgrade")
    .header("Upgrade", "websocket")
    .header("Host", host)
    .body(())?;
```

#### 8.2 Server 端（ws/agent.rs）

修改 handler 签名，从 `HeaderMap` 提取 token：

```rust
pub async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<OptionalWsQuery>,  // 兼容旧版
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError>
```

Token 提取优先级：
1. `Authorization: Bearer xxx` header（新方式）
2. `?token=xxx` query param（旧方式，兼容期内支持，记录 deprecation warning）

```rust
fn extract_agent_token(headers: &HeaderMap, query: &OptionalWsQuery) -> Option<String> {
    // 优先 header
    if let Some(auth) = headers.get("authorization") {
        if let Ok(val) = auth.to_str() {
            if let Some(token) = val.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }
    // 回退 query param（deprecation）
    if let Some(ref token) = query.token {
        tracing::warn!("Agent using deprecated query param token, please upgrade agent");
        return Some(token.clone());
    }
    None
}
```

#### 8.3 向后兼容

`WsQuery` 改为 `OptionalWsQuery`，`token` 字段改为 `Option<String>`。短期内同时支持两种方式。下个大版本移除 query param 支持。

---

## 环境变量与配置变更汇总

| 配置路径 | 环境变量 | 默认值 | 用途 |
|---------|---------|-------|------|
| `server.trusted_proxies` | `SERVERBEE_SERVER__TRUSTED_PROXIES` | `[]`（空，不信任代理） | 信任的反向代理 CIDR 列表 |
| `upgrade.release_base_url` | `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `https://github.com/ZingerLittleBee/ServerBee/releases` | Agent 升级发布源 |

按 CLAUDE.md 要求，实施时需同步更新 `ENV.md` 和 `apps/docs/content/docs/{en,cn}/configuration.mdx`。

## 新增依赖

| Crate | 用途 | Crate 位置 |
|-------|------|-----------|
| `ipnet` | CIDR 匹配（信任代理判断） | `crates/server/Cargo.toml` |

## 涉及文件汇总

| 文件 | 改动类型 |
|------|---------|
| `crates/server/src/router/mod.rs` | 删除 CorsLayer |
| `crates/server/src/router/utils.rs` | **新建**，统一 `extract_client_ip` |
| `crates/server/src/router/api/auth.rs` | 替换 `extract_client_ip` 调用，添加 `ConnectInfo` extractor |
| `crates/server/src/router/api/file.rs` | 同上 |
| `crates/server/src/router/api/server.rs` | 同上 + 重写 `trigger_upgrade` handler |
| `crates/server/src/router/api/agent.rs` | 替换 `extract_client_ip` 调用 |
| `crates/server/src/router/api/oauth.rs` | 替换内联 IP 提取为统一 `extract_client_ip` |
| `crates/server/src/router/ws/agent.rs` | Token 从 header 提取 + 兼容 query + 配置 max_message_size |
| `crates/server/src/router/ws/browser.rs` | 配置 max_message_size |
| `crates/server/src/router/ws/terminal.rs` | 配置 max_message_size |
| `crates/server/src/router/ws/docker_logs.rs` | 配置 max_message_size |
| `crates/server/src/state.rs` | DashMap 过期清理逻辑 |
| `crates/server/src/service/record.rs` | 重写 `aggregate_hourly`（SQL 聚合 + upsert） |
| `crates/server/src/migration/` | 新增 migration（去重 + 唯一索引） |
| `crates/server/src/config.rs` | `ServerConfig` 新增 `trusted_proxies` 字段；新增 `[upgrade]` 配置段 |
| `crates/server/src/service/agent_manager.rs` | `AgentConnection` 新增 `os`/`arch` 字段，供 upgrade 平台映射使用 |
| `crates/server/Cargo.toml` | 新增 `ipnet` 依赖 |
| `crates/common/src/` | `ServerMessage::Upgrade` 增加 `sha256` 字段 |
| `crates/agent/src/reporter.rs` | WS header 认证 + `perform_upgrade` 重写 |
| `crates/agent/src/file_manager.rs` | `getpwuid_r` / `getgrgid_r` 替换 |
| `apps/web/src/hooks/use-terminal-ws.ts` | try/catch JSON.parse |
| `apps/web/src/hooks/use-servers-ws.ts` | type guard 替换 as 断言 |
| `apps/web/openapi.json` | 重新生成（`bun run generate:api-types`） |
| `apps/web/src/lib/api-types.ts` | 重新生成（同上） |
| `.github/workflows/release.yml` | 新增 checksums.txt 生成步骤 |
| `ENV.md` | 新增配置项文档 |
| `apps/docs/content/docs/{en,cn}/configuration.mdx` | 同步更新 |

## 验证清单

实施完成后必须通过以下验证：

```bash
# Rust 编译 + 测试
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings

# 前端
cd apps/web
bun run typecheck
bun x ultracite check
bun run test

# OpenAPI 类型同步
bun run generate:api-types
# 确认 openapi.json 和 api-types.ts 中 UpgradeRequest 不再包含 download_url
```

**定向回归测试**：
- `aggregate_hourly`：验证幂等性（连续两次调用不产生重复记录），验证时间戳对齐到整点
- WS type guard：验证所有 10 种消息类型均能通过 guard，未知类型被拦截并 warn
- WS 畸形 payload：验证以下 payload 均被静默拦截而非抛异常：
  - `{ type: 'full_sync', servers: [null] }` — null 元素
  - `{ type: 'docker_event', server_id: 'x', event: null }` — typeof null === 'object' 陷阱
  - `{ type: 'network_probe_update', server_id: 'x', results: [null] }` — null 元素
  - `{ type: 'docker_update', server_id: 'x', containers: [null] }` — null 元素
  - `{ type: 'update' }` — 缺少 servers 字段
- Terminal WS 畸形 payload：验证以下均不抛异常：
  - `{ type: 'output', data: 123 }` — data 非字符串
  - `{ type: 'output', data: '!!!invalid-base64' }` — 非法 base64
  - `{ type: 'output' }` — 缺少 data 字段
- 速率限制：验证伪造 `X-Forwarded-For` 不再绕过限制（需配合 `trusted_proxies` 配置测试）
- Agent Token：验证 `Authorization: Bearer` header 认证成功，旧版 query param 仍兼容
