# ServerBee 全栈代码审查报告

**日期**: 2026-03-26
**审查团队**: 4 位专业角色并行审查

| 角色 | 审查范围 |
|------|---------|
| Rust Server 架构师 | `crates/server/` — Router/Service/Task/Middleware/Migration |
| Rust Agent 系统工程师 | `crates/agent/` — Collector/Reporter/Pinger/Terminal/File |
| 前端工程师 | `apps/web/` — 路由/组件/Hooks/性能/主题/类型安全 |
| 安全工程师 | 全栈安全 — 认证/RBAC/Capabilities/输入校验/高危功能 |

---

## 一、高优先级（应立即修复）— 7 项

### 1. Upgrade 机制：download_url 无校验 + 校验和可选

**文件**: `crates/server/src/router/api/server.rs:444-471` | `crates/agent/src/reporter.rs:1579-1653`

`trigger_upgrade` 端点的 `download_url` 完全由 admin 提交，未经任何 URL 格式/scheme/域名校验，直接透传给 Agent 执行下载。Agent 侧校验和仅在响应头 `x-checksum-sha256` 存在时才验证，否则直接替换二进制并重启。

**攻击场景**：Admin 账户被攻破后，攻击者可指定任意 URL（含内网地址）触发 SSRF，或推送恶意二进制完全替换 Agent 可执行文件。

**修复建议**：
- 服务端强制 URL 白名单（仅允许 `https://` + 受信任域名）
- `trigger_upgrade` 请求体中要求附带 `sha256` 字段，Agent 侧改为**必须**校验

### 2. WebSocket 消息大小未实际强制执行

**文件**: `crates/common/src/constants.rs:10` | `crates/server/src/router/ws/agent.rs`

`MAX_WS_MESSAGE_SIZE` 仅在 constants.rs 定义，**在所有 WebSocket 处理器中从未被实际配置或检查**。

**攻击场景**：持有合法 Token 的攻击者可发送无限大小的 WS 消息，触发服务端 OOM。

**修复建议**：在 `WebSocketUpgrade::on_upgrade()` 调用前配置 `.max_message_size(MAX_WS_MESSAGE_SIZE)`。

### 3. CORS 全开放策略

**文件**: `crates/server/src/router/mod.rs:17-20`

```rust
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any);
```

`allow_origin(Any)` 配合 API Key 认证（`X-API-Key` header）暴露全部 API 给任意域。

**修复建议**：改为仅允许 `config.auth.allowed_origins` 白名单。

### 4. 速率限制可被 X-Forwarded-For 欺骗绕过

**文件**: `crates/server/src/router/api/auth.rs:594-605`

```rust
fn extract_client_ip(headers: &HeaderMap) -> String {
    headers.get("x-forwarded-for")  // 无条件信任第一跳
        .map(|s| s.split(',').next()....)
        ...
}
```

攻击者可在每次登录尝试时携带不同的伪造 IP，完全绕过 15 分钟限制，实施无限制暴力破解。额外问题：DashMap 中的过期条目**从不主动清理**，长期运行内存持续增长。

**修复建议**：
- 仅在受信任反向代理之后才读取该 header（通过配置项控制）
- 默认使用 TCP 连接的真实 `ConnectInfo<SocketAddr>`
- 定期清理过期 DashMap 条目

### 5. Agent `getpwuid`/`getgrgid` 非重入版本在 async 环境下存在 UB

**文件**: `crates/agent/src/file_manager.rs:628-650`

`libc::getpwuid` / `libc::getgrgid` 返回指向静态缓冲区的指针，在多线程环境下存在数据竞争。Tokio 的 `spawn_blocking` 或并发 `list_dir` 调用可能同时触发这两个 unsafe 函数。

**修复建议**：改用 `getpwuid_r` / `getgrgid_r` 重入版本。

### 6. `aggregate_hourly` 无重复插入保护

**文件**: `crates/server/src/service/record.rs:303`

```rust
hourly.insert(db).await?;  // 无 upsert/冲突检测
```

若服务器重启或任务并发执行，同一 (server_id, time) 可能产生重复 hourly 记录。

**修复建议**：改用 `INSERT OR REPLACE` 或插入前先查重。

### 7. 前端 WebSocket 消息处理缺少健壮性

**文件**: `apps/web/src/hooks/use-terminal-ws.ts:35` | `apps/web/src/hooks/use-servers-ws.ts:155`

- Terminal WS 的 `JSON.parse(event.data)` 无 try/catch，服务端发送格式异常消息时导致未捕获异常
- 主 WS `const msg = raw as WsMessage` 仅做编译期断言，无运行时 schema 校验

**修复建议**：Terminal WS 添加 try/catch；主 WS 添加运行时类型守卫或 Zod 校验。

---

## 二、中优先级（建议尽快修复）— 12 项

### 1. Agent Token 明文暴露在 WebSocket URL 中

**文件**: `crates/agent/src/reporter.rs:1325` | `crates/server/src/router/ws/agent.rs:26-31`

Token 出现在 URL query param 中，会被服务器访问日志、代理日志记录。

**建议**：改用 WebSocket 握手阶段的 `Authorization` 请求头或首个消息帧传递 Token。

### 2. 认证性能：每次请求 3 次 DB 查询 + `/api/auth/me` 实时 argon2

**文件**: `crates/server/src/service/auth.rs:143-178` | `crates/server/src/router/api/auth.rs:248`

- `validate_session` 执行 3 次查询（SELECT session → UPDATE expires → SELECT user）
- `/api/auth/me` 对 admin 用户每次调用都执行 argon2 哈希验证（CPU 密集）

**建议**：合并 JOIN 查询；在 login 时一次性判断 `is_default_password` 写入 session。

### 3. `batch_update_capabilities` 无事务保护

**文件**: `crates/server/src/router/api/server.rs:528-593`

循环中对每个 server 单独 UPDATE，中途出错导致部分更新。

**建议**：包裹在数据库事务中。

### 4. `aggregate_hourly` 全量加载原始记录到内存

**文件**: `crates/server/src/service/record.rs:204-209`

100 个节点 × 60 条/小时 = 6000 行全部加载到内存。

**建议**：改用 SQL `GROUP BY server_id` + `AVG()` 聚合查询，计算下推到数据库。

### 5. 文件读取/列举操作 Member 角色可访问

**文件**: `crates/server/src/router/api/mod.rs:51`

`list_files`、`stat_file`、`read_file`、`download_file` 四个端点在 `require_admin` 外。

**建议**：如仅限管理员，将 `file::read_router()` 移入 `require_admin` 块内。

### 6. `extract_client_ip` 在 4 个文件中重复定义

**文件**: `router/api/agent.rs`, `auth.rs`, `server.rs`, `file.rs`

同一函数复制了 4 份，实现略有差异。

**建议**：提取到 `router/utils.rs` 或 `middleware/` 共享模块。

### 7. 前端 queryHash 硬编码比较

**文件**: `apps/web/src/hooks/use-realtime-metrics.ts:103`

```ts
if (event.query.queryHash !== '["servers"]') { return }
```

直接比对 TanStack Query 的内部 hash 字符串，版本升级后可能静默失效。

**建议**：改为 `queryKey` 比较。

### 8. `custom_css` 直接注入 style 标签

**文件**: `apps/web/src/routes/status.$slug.tsx:237`

将后端返回的 CSS 内容直接注入公开状态页面，存在 CSS exfiltration 风险。

**建议**：服务端做 CSS 内容白名单过滤，或文档中注明此为受信任管理员功能。

### 9. 终端主题与色彩主题系统脱节

**文件**: `apps/web/src/components/terminal/terminal-view.tsx:31-55`

Tokyo Night 配色硬编码，与 8 套色彩主题完全无关。

**建议**：从当前 colorTheme 的 CSS 变量中提取终端配色。

### 10. Widget 类型 13 处双重断言

**文件**: `apps/web/src/components/widget-renderer.tsx:88-112`

`as unknown as` 完全绕过类型系统。

**建议**：让 `parseConfig` 接受泛型参数或使用 Zod schema 验证。

### 11. 缺少安全响应头

**文件**: `crates/server/src/router/mod.rs`

无 CSP、X-Frame-Options、X-Content-Type-Options、Referrer-Policy。

**建议**：添加 `tower_http::set_header::SetResponseHeaderLayer` 中间件。

### 12. 服务端文件上传无大小限制

**文件**: `crates/server/src/router/api/file.rs:813-870`

Admin 用户上传文件时，服务端先无限写入 temp 文件，Agent 之后才校验大小。

**建议**：流式写入时检查累积 `file_size`，超过阈值提前终止。

---

## 三、低优先级建议

### Agent 端

| # | 文件 | 问题 |
|---|------|------|
| 1 | `reporter.rs:1574` | `fetch_external_ip` 无响应体大小限制，应限制 64 字节 |
| 2 | `file_manager.rs:502-526` | upload chunk 不验证总写入大小，`UploadState.size` 为死代码 |
| 3 | `reporter.rs:1068` | Traceroute 目标校验拒绝合法下划线 hostname |
| 4 | `probe_utils.rs:112` | HTTP 探针 `danger_accept_invalid_certs(true)` 无配置控制 |
| 5 | `reporter.rs:408` | `handle_server_message` 13 个参数，建议封装 `SessionState` |
| 6 | `register.rs:49` | 字符串操作修改 TOML 文件，特殊字符可能产生非法 TOML |
| 7 | `docker/mod.rs:24` | `DockerManager::stats_interval` 为死代码 |
| 8 | `network_prober.rs:285` | `probe_type_to_cap` 与 common crate 重复定义 |
| 9 | `collector/disk.rs` | `used()` 和 `total()` 各自独立调用 `Disks::new_with_refreshed_list()` |
| 10 | `collector/load.rs` | 三次调用 `System::load_average()`，应合并 |
| 11 | `pinger.rs:98` | Ping 超时硬编码 10 秒，无法配置 |
| 12 | `file_manager.rs:203` | `list_dir_entries` 无条目数量上限 |
| 13 | `reporter.rs:1588` | `perform_upgrade` 下载无超时 |

### Server 端

| # | 文件 | 问题 |
|---|------|------|
| 1 | `router/ws/agent.rs:244,780` | `RwLock::read().unwrap()` 锁中毒可能导致 panic |
| 2 | `router/api/agent.rs:89` | Auto-discovery key 非常量时间比较 |
| 3 | `router/api/auth.rs:362` | 新密码无最小长度/复杂度校验 |
| 4 | `error.rs:13` | `PaginatedResponse<T>` 是死代码 |
| 5 | `main.rs:68-84` | 后台任务缺乏优雅关闭协调（无 CancellationToken） |
| 6 | `service/record.rs:283` | `aggregate_hourly` 时间戳未整点对齐 |
| 7 | `state.rs:38` | OAuth state token 内存泄漏（无 TTL 清理） |
| 8 | `router/api/auth.rs:149` | Session Cookie Secure 标志默认可能未启用 |

### 前端

| # | 文件 | 问题 |
|---|------|------|
| 1 | `components/theme-provider.tsx:174` | `loadThemeCSS` 缺少 `.catch()` 错误处理 |
| 2 | `routes/_authed/servers/$id.tsx:443` | 能力位运算用魔法数 `1`，未用 `hasCap()` |
| 3 | `hooks/use-network-realtime.ts` | 使用 `window.dispatchEvent(CustomEvent)` 做跨 hook 通信 |
| 4 | `lib/api-client.ts:32` | 204 返回 `undefined as T` 类型不安全 |
| 5 | `lib/i18n.ts` | i18n locale 全量 eager import，建议 lazy loading |
| 6 | 整体 | 缺少根级 React ErrorBoundary |
| 7 | `components/theme-provider.tsx:179` | 快捷键 `d` 可能与 xterm 冲突 |
| 8 | `package.json` | `next-themes` 是未使用的死依赖 |
| 9 | 多文件 | 多处 toast 消息未国际化（硬编码英文） |
| 10 | `hooks/use-docker-subscription.ts:14` | cleanup 存在 stale closure |
| 11 | `hooks/use-file-api.ts:173` | 文件上传手动复制 api-client 解包逻辑 |
| 12 | `routes/_authed/servers/$id.tsx:349` | 双重 cast `as unknown as` 绕过类型兼容 |

---

## 四、代码亮点

四位审查者共同认可的优秀设计：

1. **Capability 双端校验**：Server + Agent 都做权限检查，纵深防御设计出色
2. **文件路径遍历防护**：`canonicalize()` + `starts_with()` + 15+ 单元测试
3. **WsClient 重连设计**：指数退避 + Jitter + 连接状态管理，前后端一致
4. **认证架构清晰**：三路认证 + RBAC read/write 分层 + require_admin 层级正确
5. **API Key 前缀索引**：先缩小候选集再 argon2 验证，避免全表扫描
6. **实时数据节流**：WeakMap + 2s 渲染节流 + tick 触发，高性能设计
7. **Docker 故障降级**：连接失败不影响主循环，30s 自动重试
8. **AppError 统一错误处理**：错误类型映射 + HTTP 状态码 + 结构化 JSON 响应
9. **2FA pending_totp 设计**：服务器端保存 secret + 10 分钟 TTL，避免客户端伪造
10. **Traceroute 输入校验**：白名单字符集有效防止命令注入，测试覆盖充分
11. **Bundle 分割合理**：xterm / recharts 独立 chunk，PWA 策略正确（API NetworkOnly）
12. **Widget ErrorBoundary**：每个 dashboard widget 独立错误边界 + resetKey 支持

---

## 五、建议修复优先级

| 批次 | 范围 | 包含项 |
|------|------|--------|
| **第一批（安全关键）** | 立即 | Upgrade URL 校验 + 强制校验和、WS 消息大小限制、CORS 收紧、速率限制修复 |
| **第二批（稳定性）** | 1-2 周 | libc 重入修复、聚合幂等保护、前端 WS 错误处理、Agent Token 传输方式 |
| **第三批（代码质量）** | 迭代优化 | 认证性能优化、类型安全改进、终端主题统一、安全响应头、i18n 补全 |
