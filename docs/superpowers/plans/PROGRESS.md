# ServerBee 实现进度

> 最后更新: 2026-03-28

## 总览

| Plan | 名称 | 状态 | Commits |
|------|------|------|---------|
| Plan 1 | Foundation (Workspace + DB + Auth + API) | **已完成** | 6 |
| Plan 2 | Agent (采集 + 上报 + 注册) | **已完成** | 1 |
| Plan 3 | Real-time (WS + 后台任务) | **已完成** | 1 |
| Plan 4 | Frontend (路由 + 仪表盘 + 详情) | **已完成** | 2 |
| P1-a | 告警 + 通知 + 远程命令 | **已完成** | 1 (`01cb970`) |
| P1-b | Ping 探测 + Web 终端 | **已完成** | 1 (`dd1dca5`) |
| P1-c | GeoIP + GPU + OpenAPI | **已完成** | 1 (`e334e74`) |
| P1-d | OAuth + 2FA | **已完成** | 1 (`00f5704`) |
| P2-a+review | 权限 + 审计 + 安全加固 | **已完成** | 1 (`020190b`) |
| P2-b/c/d/e | 状态页 + 计费 + 升级 + 备份 | **已完成** | 1 (`6cb0f6a`) |
| P3-a | 用户管理 + 缺失 API | **已完成** | 2 (`a464801`, `601f80b`) |
| P3-b | 前端 UI 完善 | **已完成** | 3 (`044e568`, `3f33de9`, `1bea44d`) |
| P3-c | 测试 | **已完成** | 2 (`7c0d681`, `3244eac`) |
| P3-d | Agent 完善 | **已完成** | 1 (`9d5835e`) |
| P3-e | 性能优化 | **已完成** | 2 (`7c13b3d`) |
| P3-f | CI/CD + 部署文档 | **已完成** | 2 (`e6fee1a`, `03cd713`) |
| P4 | 端到端验证 + 上线前加固 | **已完成** | 1 (`51e8b40`) |
| P5 | Agent Capability Toggles | **已完成** | 22 (`bfc7d14`..`56c6058`) |

| P6 | 文件管理 | **已完成** | 24 tasks |
| P7 | Docker 容器监控 | **已完成** | 25 tasks, 17 commits |
| P8 | 定时任务 (Scheduled Tasks) | **已完成** | 1 (`9a27711`) |
| P9 | 服务监控 (Service Monitor) | **已完成** | 10 commits (`f02fd23`..`dcca19e`) |
| P10 | 流量统计 (Traffic Statistics) | **已完成** | 3 commits (`846bd73`..`f28a696`) |
| P11 | IP 变更通知 (IP Change Notification) | **已完成** | 8 tasks |
| P12 | 磁盘 I/O 监控 (Disk I/O Monitoring) | **已完成（含本地构建修复 + P19 跨平台）** | 1 (`1a6d1da`) |
| P13 | 三网 Ping + Traceroute | **已完成** | 3 commits |
| P14 | 多主题 + 品牌定制 | **已完成** | 2 commits |
| P15 | 状态页增强 | **已完成** | 4 commits |
| P16 | 移动端响应式 + PWA | **已完成** | 1 commit |
| P17 | 自定义仪表盘 (Custom Dashboard) | **已完成** | 17 commits |
| P18 | Uptime 90 天时间线 | **已完成** | 9 commits (`0034346`..`e1e4ae9`) |
| P19 | 跨平台磁盘 I/O (Cross-Platform Disk I/O) | **已完成** | 3 commits (`a1996e1`..`a87ace7`) |

| P20 | 安全加固 Round 2 | **已完成** | 14 commits (`ae06e58`..`c25790f`) |

**P0~P20 全部完成。**
**自动化测试:** Rust (common 43 + agent 56 + server unit 238 + server integration 40 + docker 4) + 220 前端 vitest = **597 tests 全部通过**。

---

## Plan 1: Foundation

**文件**: `docs/superpowers/plans/2026-03-12-plan-1-foundation.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Initialize Cargo Workspace | **done** |
| T2 | Define Common Types (types.rs, constants.rs) | **done** |
| T3 | Define Protocol Messages (protocol.rs) | **done** |
| T4 | Create sea-orm Entities (20 个实体文件) | **done** |
| T5 | Create sea-orm Migrations | **done** |
| T6 | Server Configuration (figment) | **done** |
| T7 | AppState + Database Bootstrap | **done** |
| T8 | Auth Service (argon2, sessions, API keys) | **done** |
| T9 | Auth Middleware | **done** |
| T10 | Error Handling + Response Types | **done** |
| T11 | Auth API Endpoints | **done** |
| T12 | Server Management Service + API | **done** |
| T13 | Groups + Config + Settings API | **done** |
| T14 | Record Service | **done** |
| T15 | Agent Registration Endpoint | **done** |
| T16 | Admin Init + Startup Integration | **done** |
| T17 | OpenAPI Documentation | **done** (P1-c 实现) |
| T18 | CORS + Logging Middleware | **done** |

## Plan 2: Agent

**文件**: `docs/superpowers/plans/2026-03-12-plan-2-agent.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Agent Configuration (figment) | **done** |
| T2 | System Collector (cpu/mem/disk/net/load/process/temp) | **done** |
| T3 | Agent Registration (HTTP) | **done** |
| T4 | WebSocket Reporter (重连 + 心跳 + 命令执行) | **done** |
| T5 | Agent Main Entry Point | **done** |

## Plan 3: Real-time

**文件**: `docs/superpowers/plans/2026-03-12-plan-3-realtime.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | AgentManager (DashMap 连接管理) | **done** |
| T2 | Agent WebSocket Handler | **done** |
| T3 | Browser WebSocket Handler | **done** |
| T4 | Metric Recording Task (每 60s) | **done** |
| T5 | Hourly Aggregation Task | **done** |
| T6 | Data Cleanup Task | **done** |
| T7 | Offline Detection Task (每 10s) | **done** |
| T8 | Session Cleanup Task | **done** |

## Plan 4: Frontend

**文件**: `docs/superpowers/plans/2026-03-12-plan-4-frontend.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Install Dependencies | **done** |
| T2 | Configure TanStack Router | **done** |
| T3 | API Client | **done** |
| T4 | Auth Hook + Login Page | **done** |
| T5 | Authenticated Layout Guard | **done** |
| T6 | WebSocket Client | **done** |
| T7 | Servers WebSocket Hook | **done** |
| T8 | Dashboard Page (服务器卡片网格) | **done** |
| T9 | Server Detail Page (指标图表) | **done** |
| T10 | Sidebar + Header Layout | **done** |
| T11 | Settings + API Key Management | **done** |
| T12 | Build Verification | **done** |

## P1-a: 告警 + 通知 + 远程命令

| Task | 名称 | 状态 |
|------|------|------|
| T1 | NotificationService (Webhook/Telegram/Bark 渠道) | **done** |
| T2 | Notification CRUD API (10 个端点) | **done** |
| T3 | Notification Group CRUD API | **done** |
| T4 | AlertService (规则评估 + 状态管理) | **done** |
| T5 | Alert Rule CRUD API (5 个端点) | **done** |
| T6 | Alert Evaluator 后台任务 (每 60s) | **done** |
| T7 | Task (远程命令) API (3 个端点) | **done** |
| T8 | 通知管理前端页面 | **done** |
| T9 | 告警管理前端页面 | **done** |
| T10 | 侧边栏导航更新 | **done** |
| T11 | Clippy 全量修复 (7 个 collapsible_if) | **done** |
| T12 | Entity Serialize derive (4 个实体) | **done** |

## P1-b: Ping 探测 + Web 终端

| Task | 名称 | 状态 |
|------|------|------|
| T1 | PingService CRUD + agent 同步 | **done** |
| T2 | Ping API Router (6 个端点) | **done** |
| T3 | Agent PingManager (ICMP/TCP/HTTP 三种探针) | **done** |
| T4 | Ping 任务前端页面 + 侧边栏导航 | **done** |
| T5 | Protocol 扩展 (TerminalOpen/Input/Resize/Output/Started/Error) | **done** |
| T6 | Agent TerminalManager (portable-pty PTY 会话管理) | **done** |
| T7 | Server Terminal WS 代理 (/api/ws/terminal/:server_id) | **done** |
| T8 | AgentManager 终端 session 路由表 | **done** |
| T9 | 前端 xterm.js 终端组件 + WS Hook | **done** |
| T10 | 终端路由页面 + Server Detail Terminal 按钮 | **done** |
| T11 | ping_record Entity Serialize derive | **done** |

## P1-c: GeoIP + GPU + OpenAPI

| Task | 名称 | 状态 |
|------|------|------|
| T1 | GeoIpService (maxminddb MMDB 加载 + IP 查询) | **done** |
| T2 | GeoIP 集成: Agent SystemInfo → region/country_code 写入 server | **done** |
| T3 | GPU 采集: nvml-wrapper feature-gated, GpuReport 上报 | **done** |
| T4 | GPU Record 持久化 + API (GET /servers/:id/gpu-records) | **done** |
| T5 | OpenAPI: 全部 44 个 API 端点 utoipa::path 注解 | **done** |
| T6 | OpenAPI: Entity/DTO ToSchema 注解 (20+ 类型) | **done** |
| T7 | OpenAPI: ApiDoc + Swagger UI 挂载 (/swagger-ui) | **done** |
| T8 | Clippy 0 warnings 验证 | **done** |

## P1-d: OAuth + 2FA

| Task | 名称 | 状态 |
|------|------|------|
| T1 | oauth_account Entity + Migration (含 FK + CASCADE) | **done** |
| T2 | OAuth Config (GitHub/Google/OIDC) | **done** |
| T3 | OAuthService (build_client, fetch_user_info, find_or_create_user) | **done** |
| T4 | AuthService 2FA 方法 (generate_totp_secret, verify_totp, enable/disable/has_2fa) | **done** |
| T5 | Login 修改支持 totp_code + 2fa_required 错误 | **done** |
| T6 | 2FA API 端点 (setup/enable/disable/status, secret 服务端暂存) | **done** |
| T7 | OAuth Account API 端点 (list/unlink) | **done** |
| T8 | OAuth Flow 端点 (authorize + callback, CSRF state 验证) | **done** |
| T9 | OAuth providers 端点 (GET /api/auth/oauth/providers) | **done** |
| T10 | OpenAPI 更新 | **done** |
| T11 | 前端 Security 页面 (2FA + 修改密码 + OAuth 账号) | **done** |
| T12 | 前端 Login 页面 (2FA 输入 + 动态 OAuth 按钮) | **done** |
| T13 | 侧边栏导航更新 | **done** |
| T14 | 代码审查修复 (CSRF、角色、cookie、TOTP 暂存等 10 项) | **done** |

## P2-a: 多用户权限 + 审计日志

| Task | 名称 | 状态 |
|------|------|------|
| T1 | require_admin 中间件 (role == "admin" 校验) | **done** |
| T2 | Admin-only 路由保护 (settings/notification/alert/task/audit) | **done** |
| T3 | AuditService (log + list) | **done** |
| T4 | 审计日志 API (GET /api/audit-logs, admin only) | **done** |
| T5 | 关键操作审计记录 (login/password/2fa) | **done** |
| T6 | audit_log entity ToSchema + OpenAPI | **done** |
| T7 | 前端审计日志页面 (分页表格) | **done** |
| T8 | 侧边栏导航更新 (Audit Logs) | **done** |
| T9 | OAuth 新用户角色 member (非 admin) | **done** |
| T10 | User entity 添加 OauthAccounts 关系 | **done** |
| T11 | 编译验证 | **done** |

## P2-b: 公开状态页

| Task | 名称 | 状态 |
|------|------|------|
| T1 | StatusPageResponse + StatusServer + StatusMetrics + StatusGroup 类型定义 | **done** |
| T2 | GET /api/status 公开端点 (无需认证, 返回非隐藏服务器 + 在线指标) | **done** |
| T3 | OpenAPI 更新 (status tag + 4 个 schema) | **done** |
| T4 | 前端 /status 页面 (分组展示 + 进度条 + 自动刷新 10s) | **done** |
| T5 | 编译验证 (tsc + vite build) | **done** |

## P2-c: 计费信息管理

| Task | 名称 | 状态 |
|------|------|------|
| T1 | UpdateServerInput 添加计费字段 (price/billing_cycle/currency/expired_at/traffic_limit) | **done** |
| T2 | ServerService::update_server 处理计费字段更新 | **done** |
| T3 | 到期告警规则类型 (expiration) + check_expiration 评估器 | **done** |
| T4 | 前端 BillingInfoBar 组件 (价格/到期/流量展示) | **done** |
| T5 | 前端 ServerEditDialog 编辑对话框 (基础信息 + 计费信息) | **done** |
| T6 | 编译验证 (cargo + tsc + vite) | **done** |

## P2-d: Agent 自动更新

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Protocol Upgrade 消息 (已存在于 protocol.rs) | **done** |
| T2 | Agent perform_upgrade: 下载 → sha256 校验 → 替换二进制 → 重启 | **done** |
| T3 | Server API: POST /api/servers/:id/upgrade (发送 Upgrade 命令到 Agent) | **done** |
| T4 | OpenAPI 更新 (UpgradeRequest schema) | **done** |
| T5 | Agent 添加 sha2 依赖 | **done** |
| T6 | 编译验证 | **done** |

## P2-e: 备份恢复

| Task | 名称 | 状态 |
|------|------|------|
| T1 | POST /api/settings/backup — VACUUM INTO 生成一致性备份并下载 | **done** |
| T2 | POST /api/settings/restore — 上传 SQLite 文件, 校验文件头, 替换数据库 | **done** |
| T3 | OpenAPI 更新 | **done** |
| T4 | 编译验证 | **done** |

## P2-review: 代码审查修复

| Task | 名称 | 状态 |
|------|------|------|
| T1 | [C1] Session cookie 添加 Secure 标志 (可配置 secure_cookie) | **done** |
| T2 | [C2] Login 端点限流 (DashMap IP 计数, 15min 窗口) | **done** |
| T3 | [C3] Login 提取真实 IP/User-Agent (x-forwarded-for/x-real-ip) | **done** |
| T4 | [I1] pending_totp 过期清理 (session_cleaner 任务) | **done** |
| T5 | [I4] change_password/totp_enable/totp_disable 审计日志提取真实 IP | **done** |
| T6 | [I5] 非管理员用户隐藏 admin-only 侧边栏链接 | **done** |
| T7 | [I6] OAuth 自动注册配置开关 (allow_registration, 默认 false) | **done** |
| T8 | login_rate_limit 过期清理 (session_cleaner 任务) | **done** |
| T9 | 编译验证 | **done** |

---

## 已实现的文件清单

### Rust (88 files)

**crates/common/src/** (4 files)
- `lib.rs`, `types.rs`, `constants.rs`, `protocol.rs`

**crates/server/src/** (67 files)
- `main.rs`, `lib.rs`, `config.rs`, `state.rs`, `error.rs`, **`openapi.rs`**
- `entity/` (21 files): user, session, api_key, server, server_group, server_tag, record, record_hourly, gpu_record, config, alert_rule, alert_state, notification, notification_group, ping_task, ping_record, task, task_result, audit_log, **oauth_account**
- `migration/` (4 files): mod.rs, m20260312_000001_init.rs, **m20260312_000002_oauth.rs**, **m20260314_000001_capabilities.rs**
- `middleware/` (2 files): mod.rs, auth.rs
- `service/` (13 files): mod.rs, auth.rs, server.rs, config.rs, record.rs, agent_manager.rs, **notification.rs**, **alert.rs**, **ping.rs**, **geoip.rs**, **oauth.rs**, **audit.rs**, **user.rs**
- `router/api/` (13 files): mod.rs, auth.rs, server.rs, server_group.rs, setting.rs, agent.rs, **notification.rs**, **alert.rs**, **task.rs**, **ping.rs**, **oauth.rs**, **audit.rs**, **user.rs**
- `router/ws/` (4 files): mod.rs, agent.rs, browser.rs, **terminal.rs**
- `router/` (2 files): mod.rs, static_files.rs
- `task/` (7 files): mod.rs, record_writer.rs, aggregator.rs, cleanup.rs, offline_checker.rs, session_cleaner.rs, **alert_evaluator.rs**

**crates/agent/src/** (15 files)
- `main.rs`, `config.rs`, `register.rs`, `reporter.rs`, **`pinger.rs`**, **`terminal.rs`**
- `collector/` (9 files): mod.rs, cpu.rs, memory.rs, disk.rs, network.rs, load.rs, process.rs, temperature.rs, **gpu.rs**

### Frontend (37 files)

**apps/web/src/**
- `main.tsx`, `router.tsx`, `routeTree.gen.ts`
- `lib/`: api-client.ts, ws-client.ts, utils.ts, **capabilities.ts**, **capabilities.test.ts**
- `hooks/`: use-auth.ts, use-servers-ws.ts, use-api.ts, **use-terminal-ws.ts**
- `components/ui/`: button.tsx
- `components/layout/`: sidebar.tsx, header.tsx, theme-toggle.tsx
- `components/server/`: server-card.tsx, status-badge.tsx, metrics-chart.tsx, **server-edit-dialog.tsx**
- `components/terminal/`: **terminal-view.tsx**
- `components/`: theme-provider.tsx
- `routes/`: __root.tsx, login.tsx, **status.tsx**, _authed.tsx, index.tsx (redirect)
- `routes/_authed/`: index.tsx (dashboard), servers/$id.tsx (detail), **terminal.$serverId.tsx**
- `routes/_authed/settings/`: index.tsx, api-keys.tsx, **notifications.tsx**, **alerts.tsx**, **tasks.tsx**, **ping-tasks.tsx**, **security.tsx**, **audit-logs.tsx**, **users.tsx**, **capabilities.tsx**

### 部署 (7 files)

- `Dockerfile`, `docker-compose.yml`
- `deploy/`: install.sh, serverbee-server.service, serverbee-agent.service
- `.github/workflows/`: ci.yml, release.yml

---

## 已完成的工作

### P0: 端到端验证 + Bug 修复
- [x] 端到端集成测试: 启动 server + agent, 验证注册→连接→上报→仪表盘展示完整流程 ✅
- [x] 修复集成中发现的 9 个 bug ✅
- [x] 清理编译警告 (24 个 dead_code warnings → 0) ✅
- [x] 部署基础设施 (Docker + systemd + install.sh + CI/CD) ✅
- [x] rust-embed 嵌入前端到 server 二进制 ✅

### P1-a: 告警 + 通知 + 远程命令
- [x] 通知系统: Webhook / Telegram / Bark / **Email (SMTP)** 四种渠道 + 通知组 + 模板变量 ✅
- [x] 告警引擎: 14 种指标阈值 + 离线检测 + **流量周期告警** + AND 逻辑 + 70% 采样判定 ✅
- [x] 告警状态: DashMap 热缓存 + SQLite 持久化, always/once 两种触发模式 ✅
- [x] 后台评估任务: 每 60 秒评估全部 enabled 规则 ✅
- [x] 远程命令执行: POST /api/tasks → 通过 WS 下发到 Agent → 结果回写 DB ✅
- [x] 前端: 通知管理页 + 告警管理页 + **远程命令页** + 侧边栏导航 ✅
- [x] 代码质量: Clippy 全量 0 warnings, Ultracite lint 通过 ✅

### P1-b: Ping 探测 + Web 终端
- [x] Ping 探测: ICMP (系统 ping 命令) / TCP (TcpStream) / HTTP (reqwest) 三种探针 ✅
- [x] Ping 任务管理: CRUD API (6 端点) + Agent 同步 (PingTasksSync) + 前端管理页 ✅
- [x] Agent PingManager: 并行任务调度, 按 interval 定时探测, 结果通过 WS 上报 ✅
- [x] Web 终端协议: 新增 TerminalOpen/Input/Resize (Server→Agent) + TerminalOutput/Started/Error (Agent→Server) ✅
- [x] Agent PTY: portable-pty 管理 PTY 会话, base64 编码, 阻塞 reader 线程 + mpsc 转发 ✅
- [x] Server 终端代理: WS 端点 `/api/ws/terminal/:server_id`, session 路由, 空闲超时 10min ✅
- [x] 前端终端: xterm.js (Tokyo Night 主题) + FitAddon + WebLinksAddon + 终端路由页 ✅
- [x] Server Detail 页面: 在线服务器显示 Terminal 按钮 ✅
- [x] 代码质量: Clippy 0 warnings, TypeScript + Vite build 通过 ✅

### P1-c: GeoIP + GPU + OpenAPI
- [x] GeoIP: maxminddb MMDB 加载, Agent SystemInfo 时解析 remote_addr → region/country_code ✅
- [x] GPU 采集: nvml-wrapper 0.10, feature-gated `#[cfg(feature = "gpu")]`, GpuReport 上报 ✅
- [x] GPU 数据: gpu_record 实体 + RecordService 持久化 + API 端点 ✅
- [x] OpenAPI: utoipa v5 + utoipa-swagger-ui v9, 全部 44 个 API 端点注解 ✅
- [x] OpenAPI: 20+ 类型 ToSchema/IntoParams 注解, Swagger UI 挂载 /swagger-ui ✅
- [x] 代码质量: Clippy 0 warnings ✅

### P1-d: OAuth + 2FA
- [x] OAuth: GitHub/Google/OIDC 三种 Provider, oauth2 v4 授权码流 ✅
- [x] OAuth: 自动创建用户 (首次 OAuth 登录) + 账号关联/解关联 ✅
- [x] 2FA: TOTP (totp-rs v5), QR 码生成, 启用/禁用/状态查询 ✅
- [x] Login: 支持 totp_code 可选字段, 2FA 启用时返回 2fa_required 错误 ✅
- [x] OpenAPI: 更新 ApiDoc, 新增 2FA/OAuth 路径和模式 (共 51 个端点) ✅
- [x] 前端: Security 页面 (2FA 设置 + 修改密码 + OAuth 账号管理) ✅
- [x] 前端: Login 页面 2FA 输入 + OAuth 按钮 (GitHub/Google) ✅
- [x] 代码质量: Rust + TypeScript + Vite build 全部通过 ✅

#### 已修复的集成 Bug (P0 阶段)
1. **Server Config `Default` 导致 panic**: `DatabaseConfig::default()` 的 `max_connections=0` 导致 SQLx pool panic。修复：所有 config struct 手动实现 `Default` 使用正确的默认值。
2. **API 泄露 token_hash**: `/api/servers` 返回了 `token_hash` 和 `token_prefix`。修复：新增 `ServerResponse` DTO 过滤敏感字段。
3. **前端 API 客户端未解包 `{ data: T }`**: `api-client.ts` 返回了整个 `ApiResponse` 而非内部 `data`。修复：自动提取 `.data`。
4. **前端 Auth 字段名错误**: User 接口用 `id` 而服务端返回 `user_id`，缺少 `role`。修复。
5. **WebSocket 消息格式不匹配**: `update` 用 singular `server`（应为 `servers` 数组），`server_online/offline` 期望完整对象（实际只有 `server_id`）。修复。
6. **ServerMetrics 字段名全错**: `cpu_usage→cpu`, `memory_total→mem_total`, `network_in_speed→net_in_speed`, `load_avg→load1/5/15` 等。修复。
7. **API 路径错误**: Settings 页面用 `/api/settings/discovery`（应为 `/api/settings/auto-discovery-key`），API Keys 用 `/api/settings/api-keys`（应为 `/api/auth/api-keys`）。修复。
8. **ServerRecord 字段名错误**: `cpu_usage→cpu`, `timestamp→time`, `memory_used→mem_used` 等。修复。
9. **Server Detail 页面时间范围**: interval 参数 `1m/5m/15m/1h/6h` 改为 `raw/hourly` 匹配服务端。

---

## 未完成的工作

### 待实现: P1 剩余功能
- [x] Email 通知渠道 (lettre SMTP) ✅
- [x] 流量周期告警 (transfer_in/out/all_cycle) ✅
- [x] 远程命令前端页面 ✅
- [x] Ping 探测任务 (ICMP/TCP/HTTP) ✅
- [x] Ping 任务前端页面 ✅
- [x] Web 终端 (PTY 代理) ✅
- [x] Web 终端前端页面 ✅
- [x] 温度/GPU 采集 (Agent 端, nvml-wrapper feature-gated) ✅
- [x] GeoIP 查询 (maxminddb, agent SystemInfo 时解析) ✅
- [x] OpenAPI 文档 (utoipa v5 + Swagger UI, 44 个端点全覆盖) ✅
- [x] OAuth (GitHub, Google, OIDC) ✅
- [x] 2FA (TOTP) ✅

### 待实现: P2 功能
- [x] 多用户 (Admin/Member 角色 + require_admin 中间件) ✅
- [x] 审计日志 (AuditService + API + 前端页面) ✅
- [x] 备份恢复 (VACUUM INTO 备份 + 文件上传恢复 + SQLite 头校验) ✅
- [x] Agent 自动更新 (Server Upgrade 命令 + Agent 下载/校验/替换/重启) ✅
- [x] 公开状态页 (后端 API + 前端页面, 分组展示, 10s 自动刷新) ✅
- [x] 计费信息管理 (UpdateServerInput 计费字段 + 到期告警 + 前端编辑对话框) ✅

### 代码质量
- [x] Rust 单元测试 (100 个, 13 个模块覆盖: auth/alert/notification/record/agent_manager/server/user/ping/middleware + agent collector/pinger + common constants/protocol) ✅
- [x] Rust 集成测试 (10 个, Agent 注册→WS→上报 + 备份恢复 + 认证流程 + RBAC + CRUD + API Key + 审计日志 + 状态页 + auto-discovery) ✅
- [x] 前端测试 (52 个, 7 个测试文件: api-client/utils/ws-client/capabilities + use-auth/use-api/use-servers-ws hooks) ✅
- [x] 代码审查 (P3 代码审查, 修复 10 个 C/I 级别问题) ✅
- [x] `bun x ultracite fix` 格式化前端代码 ✅
- [x] `cargo clippy -- -D warnings` 全量通过 ✅

---

## Git Commits

```
56c6058 fix(server): write task_result on CapabilityDenied, add audit log and selective ping re-sync to update_server
c0bb58a fix(web): wrap ShieldAlert icon in span for title tooltip
f59eb98 test: add unit tests for capability helpers, protocol serialization, and frontend toggles
d247e41 chore: add batch-capabilities to OpenAPI, fix clippy collapsible-if warnings
daef9f8 feat(web): grey out exec-disabled servers and show skipped results
42ae8bc feat(web): add capabilities settings page with batch management
ed13196 feat(web): add capability toggles section to server detail page
8efa1b5 feat(web): move WS hook to global layout, add capabilities_changed + agent_info_updated handlers
d817273 feat(web): add shared capabilities constants
ef271c1 fix(test): update integration test for protocol v2 and async ping sync
6f268d4 feat(agent): filter ping tasks by capability, check CAP_TERMINAL before PTY open
f42a78c feat(agent): add capabilities enforcement — CAP_EXEC, CAP_UPGRADE, CapabilitiesSync
c9b81ad feat(server): broadcast capabilities changes and re-sync pings from update handler
697173e feat(server): filter ping tasks by CAP_PING_* capability
e76128b feat(server): filter tasks by CAP_EXEC, write synthetic results for disabled servers
2f655f5 feat(server): block terminal WS if CAP_TERMINAL disabled
76321b6 feat(server): send capabilities in Welcome, persist protocol_version, handle CapabilityDenied
28370c5 feat(server): add capabilities to API responses and batch update endpoint
b385007 feat(server): add protocol_version tracking, browser broadcast, and Forbidden(String)
68a2717 feat(server): add capabilities and protocol_version columns via migration
af8f47a feat(common): extend protocol with capability messages and version negotiation
bfc7d14 feat(common): add capability bitmap constants and helpers
1bea44d feat: add servers list page with table view and batch operations (P3-b T10)
3244eac test: add Rust integration tests and frontend Vitest tests (P3-c T5-T7)
4c0e026 docs: update progress for P4 end-to-end verification
51e8b40 fix: end-to-end verification fixes and pre-launch hardening (P4)
c806f89 docs: add pre-launch checklist and next steps to PROGRESS.md
f82eedc docs: update progress for P3-c through P3-f completion
e6fee1a chore: add Windows CI build, cargo test step, and README (P3-f)
7c13b3d perf: add Vite code splitting for xterm and recharts (P3-e)
9d5835e feat: add virtualization detection and Windows connection counting (P3-d)
7c0d681 test: add 43 unit tests for core services (P3-c)
8b330dd docs: update progress for P3-a and P3-b completion
3f33de9 feat: enhance dashboard, server detail, and monitoring UI (P3-b)
044e568 feat: add shared utils, group_id support, and WS merge fix (P3-b)
601f80b feat: add registration rate limiting, SIGTERM shutdown, and security fixes (P3-a)
a464801 feat: add user management CRUD API and frontend page (P3-a)
d8c8e9f docs: update PROGRESS.md with actual commit hashes for P1/P2 milestones
cbb7cdc docs: update implementation progress for P1 and P2 milestones
6cb0f6a feat: add public status page, billing management, and backup/restore (P2-b/c/d/e)
020190b feat: add role-based access control, audit logging, and security hardening (P2-a + P2-review)
00f5704 feat: add OAuth login and two-factor authentication (P1-d)
e334e74 feat: add GeoIP, GPU collection, and OpenAPI documentation (P1-c)
dd1dca5 feat: add ping monitoring and web terminal (P1-b)
01cb970 feat: add notification, alert, and remote command systems (P1-a)
cd89953 ci: add GitHub Actions CI and release workflows
038da71 feat: add deployment infrastructure with rust-embed, Docker, and systemd
39f7097 chore: suppress 24 dead_code warnings for P1 feature scaffolding
ae9af0c fix: resolve 9 integration bugs found during end-to-end testing
c2e93be docs: add implementation progress tracking document
a5406ac chore(web): add TanStack Router, Query, and Recharts dependencies
da1488b feat(web): add full SPA with routing, auth, dashboard, and server detail pages
9c9d965 feat: add real-time infrastructure with WebSocket handlers and background tasks
16f5fe1 feat: implement agent configuration, system collector, registration, WebSocket reporter, and main entry point
1f4778d feat: add admin init, auto-discovery key, and CORS/tracing middleware
80aaed7 feat: add API endpoints, services, and router infrastructure
5e9f88e feat: add error handling, auth service, and auth middleware
cbef840 feat: add server configuration, AppState, and database bootstrap
cd78764 feat: add sea-orm entities and initial database migration
c0dc189 feat: initialize Cargo workspace with common types and protocol messages
68cf945 docs: add 4 implementation plans for ServerBee
```

## 新增 API 端点汇总

### 通知 (Session 认证)
```
GET    /api/notifications                 列出所有通知渠道
POST   /api/notifications                 创建通知渠道
GET    /api/notifications/:id             获取通知渠道详情
PUT    /api/notifications/:id             更新通知渠道
DELETE /api/notifications/:id             删除通知渠道
POST   /api/notifications/:id/test        测试发送通知
GET    /api/notification-groups           列出所有通知组
POST   /api/notification-groups           创建通知组
PUT    /api/notification-groups/:id       更新通知组
DELETE /api/notification-groups/:id       删除通知组
```

### 告警规则 (Session|API Key 认证)
```
GET    /api/alert-rules                   列出所有告警规则
POST   /api/alert-rules                   创建告警规则
GET    /api/alert-rules/:id              获取告警规则详情
PUT    /api/alert-rules/:id              更新告警规则
DELETE /api/alert-rules/:id              删除告警规则
```

### 远程命令 (Session|API Key 认证)
```
POST   /api/tasks                         创建并下发命令
GET    /api/tasks/:id                     获取任务详情
GET    /api/tasks/:id/results             获取任务执行结果
```

### Ping 探测 (Session|API Key 认证)
```
GET    /api/ping-tasks                    列出所有 Ping 任务
POST   /api/ping-tasks                    创建 Ping 任务
GET    /api/ping-tasks/:id               获取 Ping 任务详情
PUT    /api/ping-tasks/:id               更新 Ping 任务
DELETE /api/ping-tasks/:id               删除 Ping 任务
GET    /api/ping-tasks/:id/records       获取探测记录 (?from=&to=&server_id=)
```

### Web 终端 (Session|API Key 认证)
```
WS     /api/ws/terminal/:server_id       终端 WebSocket (代理 Browser↔Agent PTY)
```

### OAuth + 2FA (Session|OAuth 认证)
```
POST   /api/auth/2fa/setup               生成 TOTP secret + QR 码
POST   /api/auth/2fa/enable              验证并启用 2FA
POST   /api/auth/2fa/disable             验证密码并禁用 2FA
GET    /api/auth/2fa/status              查询 2FA 状态
GET    /api/auth/oauth/accounts          列出已关联 OAuth 账号
DELETE /api/auth/oauth/accounts/:id      解关联 OAuth 账号
GET    /api/auth/oauth/:provider         OAuth 授权重定向
GET    /api/auth/oauth/:provider/callback OAuth 回调
```

### 服务器管理扩展 (Session|API Key 认证)
```
POST   /api/servers/:id/upgrade           触发 Agent 远程升级
PUT    /api/servers/batch-capabilities     批量更新服务器 capabilities
```

### 设置扩展 (Admin, Session|API Key 认证)
```
POST   /api/settings/backup               下载 SQLite 数据库备份
POST   /api/settings/restore              上传并恢复 SQLite 数据库
```

### 公开状态页 (无需认证)
```
GET    /api/status                        公开服务器状态页数据 (非隐藏服务器 + 在线指标)
```

### 用户管理 (Admin, Session|API Key 认证)
```
GET    /api/users                         列出所有用户
POST   /api/users                         创建用户
GET    /api/users/:id                     获取用户详情
PUT    /api/users/:id                     更新用户 (角色等)
DELETE /api/users/:id                     删除用户 (禁止删除最后 admin)
```

### 审计日志 (Admin, Session|API Key 认证)
```
GET    /api/audit-logs                    列出审计日志 (?limit=&offset=)
```

### P2-b: 公开状态页
- [x] 后端: `GET /api/status` 公开端点, 返回非隐藏服务器 + 在线状态 + 实时指标 ✅
- [x] 前端: `/status` 页面, 分组展示服务器, 进度条 (CPU/内存/磁盘), 10s 自动刷新 ✅
- [x] 代码质量: TypeScript + Vite build 通过 ✅

### P2-c: 计费信息管理
- [x] `UpdateServerInput` 新增 price/billing_cycle/currency/expired_at/traffic_limit/traffic_limit_type 字段 ✅
- [x] `ServerService::update_server` 处理计费字段更新 ✅
- [x] 到期告警: `expiration` 规则类型, 检测 expired_at 是否在 N 天内 ✅
- [x] 前端: `BillingInfoBar` 组件, 展示价格/到期时间/流量限额 ✅
- [x] 前端: `ServerEditDialog` 编辑对话框, 支持基础信息 + 计费信息编辑 ✅
- [x] 代码质量: cargo + tsc + vite 全部通过 ✅

### P2-d: Agent 自动更新
- [x] Agent: `perform_upgrade` — 下载新二进制, sha256 校验, 替换当前文件, 重启进程 ✅
- [x] Server: `POST /api/servers/:id/upgrade` — 通过 WS 发送 Upgrade 命令到在线 Agent ✅
- [x] OpenAPI: UpgradeRequest schema ✅

### P2-e: 备份恢复
- [x] `POST /api/settings/backup` — SQLite VACUUM INTO 生成一致性备份, 下载为 .db 文件 ✅
- [x] `POST /api/settings/restore` — 上传 SQLite 文件, 校验文件头 (magic bytes), 备份当前 DB, 替换 ✅

### P2-review: 代码审查修复
- [x] Session cookie 添加 `Secure` 标志 (可配置 `secure_cookie`) ✅
- [x] Login 端点限流 (DashMap IP 计数, 15 分钟窗口, login_max=5) ✅
- [x] Login/change_password/2fa 端点提取真实 IP/User-Agent ✅
- [x] `pending_totp` + `login_rate_limit` 过期清理 (session_cleaner 任务) ✅
- [x] 非管理员用户隐藏 admin-only 侧边栏链接 ✅
- [x] OAuth 自动注册配置开关 (`allow_registration`, 默认 false) ✅

### P5: Agent Capability Toggles (Per-Agent 功能开关)
- [x] 能力位图 (u32): CAP_TERMINAL=1, CAP_EXEC=2, CAP_UPGRADE=4, CAP_PING_ICMP=8, CAP_PING_TCP=16, CAP_PING_HTTP=32, CAP_DEFAULT=56 ✅
- [x] 协议版本 PROTOCOL_VERSION 1→2, SystemInfo 携带 protocol_version ✅
- [x] Server 端验证: Terminal WS 403 拦截, Task API CAP_EXEC 过滤, Ping 按 capability 过滤 ✅
- [x] Agent 端验证: Arc<AtomicU32> 本地 capabilities, CapabilitiesSync 实时更新, 拒绝执行 → CapabilityDenied ✅
- [x] 双重验证 (defense in depth): Server 拦截 + Agent 拒绝执行 ✅
- [x] 实时推送: 修改 capabilities 后 → CapabilitiesSync 到 Agent + CapabilitiesChanged 到 Browser ✅
- [x] 选择性 ping 重同步: 仅当 ping 相关 capability bits 变化时触发 ✅
- [x] 合成 task_result: Server 拦截 exit_code=-2, Agent 拒绝 exit_code=-1 ✅
- [x] 前端: Settings/Capabilities 管理页, Server Detail toggle, Tasks 灰显, 侧边栏导航 ✅
- [x] 测试: 6 个 constants 单元测试 + 5 个 protocol 序列化测试 + 3 个前端 vitest ✅
- [x] 代码审查修复: CapabilityDenied 写 task_result, audit log, selective ping re-sync, clippy collapsible-if ✅

### P3-a: 用户管理 + 缺失 API
- [x] User CRUD API: UserService + 5 个 admin-only 端点 (GET/POST /users, GET/PUT/DELETE /users/:id) ✅
- [x] 前端用户管理页面: 列表/创建/编辑角色/删除, 侧边栏导航 ✅
- [x] Agent 注册端点限流: DashMap 限流, register_max 配置, session_cleaner 清理 ✅
- [x] SIGTERM 优雅关闭: tokio::select! SIGINT+SIGTERM, #[cfg(unix)] 条件编译 ✅

### P3-b: 前端 UI 完善
- [x] Dashboard 统计卡片: 5 个 StatCard (Servers/CPU/Memory/Bandwidth/Health) ✅
- [x] Dashboard 分组展示: 按 group_id 分组 + group headers ✅
- [x] Server Card 国旗 + OS 图标: countryCodeToFlag + osIcon helper ✅
- [x] Server Detail GPU 面板: gpu-records API + GPU Usage/Temperature 图表 ✅
- [x] Server Detail 温度图表: 条件渲染 (temperature > 0) ✅
- [x] Server Detail 网络累计流量: net_in_transfer/net_out_transfer stats bar ✅
- [x] Server Detail 补充信息: ipv6, kernel_version, cpu_arch, region, agent_version ✅
- [x] Ping 结果图表: PingResultsChart 组件 + 24h 延迟面积图 + 成功率/平均延迟 ✅
- [x] 审计日志分页防闪烁: placeholderData: (prev) => prev ✅
- [x] 共享工具函数: formatBytes, formatSpeed, formatUptime, countryCodeToFlag 提取到 lib/utils.ts ✅
- [x] 代码质量: cargo check + tsc + vite build 全部通过 ✅

### P3-c: 测试
- [x] Rust 单元测试 (110 个): AuthService (19) + AlertService (15) + NotificationService (16) + RecordService (6) + AgentManager (10) + ServerService (5) + UserService (4) + PingService (3) + AuditService (3) + ConfigService (5) + middleware/auth (6) + common constants (6) + protocol (5) + agent collector (5) + agent pinger (2) ✅
- [x] Rust 集成测试 (11 个): Agent 注册→WS→上报 + Backup/Restore + Login/Logout + API Key + RBAC + Public Status + Audit Log + Notification/Alert CRUD + User CRUD + Discovery Key + Alert States ✅
- [x] 前端 Vitest 测试 (72 个): api-client (6) + utils (21) + ws-client (6) + capabilities (3) + use-auth (4) + use-api (4) + use-servers-ws (8) + use-terminal-ws (20) ✅
- [x] CI 添加 `cargo test --workspace` 步骤 ✅
- [x] E2E 浏览器测试 (31 个, agent-browser): 登录流程 + Dashboard 实时更新 + 搜索过滤 + 编辑对话框 + 时间范围切换 + 用户 CRUD + 通知/API Key 展示 + Capabilities + 公共状态页 + 主题切换 + 告警全链路 + Swagger UI + Ping 创建/结果 + 终端页面加载 ✅
- [x] Bug 修复 (5 个): 登录错误消息 + 通知密码掩码 + 告警表单扩展 + 告警状态 API/UI + Capabilities API Extension crash ✅

### P3-d: Agent 完善
- [x] 虚拟化检测: DMI 文件 + 容器检测 + systemd-detect-virt fallback ✅
- [x] Config enable_temperature/enable_gpu 开关: 已存在并验证 ✅
- [x] Windows TCP/UDP 连接数: netstat 命令 + #[cfg(target_os)] 条件编译 ✅

### P3-e: 性能优化
- [x] Vite Code Split: xterm (333KB) + recharts (354KB) 独立 chunk, 主 bundle 1124KB → 435KB ✅
- [x] 数据库查询索引: (server_id, time) 复合索引已存在 ✅
- [x] OpenAPI 类型生成: utoipa 导出 → openapi-typescript 生成 → 替换 14 个文件的手写接口 ✅

### P3-f: CI/CD + 部署文档
- [x] CI Windows 构建: x86_64-pc-windows-msvc target + .exe artifact 处理 ✅
- [x] README.md: 功能列表 + 技术栈 + 快速开始 + 配置 + 部署指南 ✅
- [x] 部署文档: OAuth/2FA/GeoIP/备份恢复配置说明 (README 内) ✅
- [x] Fumadocs 文档站: 中英文双语 16 个页面 (修复环境变量映射 + 新增 capabilities/security/status-page/admin/api-reference) ✅

## P3: 后续任务

### P3-a: 用户管理 + 缺失 API

| Task | 名称 | 状态 |
|------|------|------|
| T1 | User CRUD API (GET/POST/PUT/DELETE /api/users, admin only) | **done** |
| T2 | 前端用户管理页面 (settings/users.tsx, 列表/创建/编辑角色/删除) | **done** |
| T3 | Agent 注册端点限流 (复用 DashMap 限流, register_max 配置) | **done** |
| T4 | SIGTERM 优雅关闭 (server 当前仅处理 SIGINT, systemd 发 SIGTERM) | **done** |

### P3-b: 前端 UI 完善

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Dashboard 统计卡片 (在线/离线/总数、CPU 平均、内存平均、总带宽) | **done** |
| T2 | Dashboard 按分组展示服务器卡片 (group headers + 分组折叠) | **done** |
| T3 | Server Card 添加国旗 emoji + OS 图标 (基于 country_code/os 字段) | **done** |
| T4 | Server Detail GPU 面板 (调用 GET /api/servers/:id/gpu-records + 图表) | **done** |
| T5 | Server Detail 温度图表 (records 中 temperature 字段) | **done** |
| T6 | Server Detail 网络累计流量统计 (net_in_transfer/net_out_transfer) | **done** |
| T7 | Server Detail 补充信息 (region, agent_version, ipv6, kernel_version, cpu_arch) | **done** |
| T8 | Ping 任务结果图表 (调用 GET /api/ping-tasks/:id/records + 延迟图表) | **done** |
| T9 | 审计日志分页 placeholderData (TanStack Query keepPreviousData 防闪烁) | **done** |
| T10 | 服务器列表/管理页面 (servers/index.tsx, 表格视图 + 批量操作) | **done** |

### P3-c: 测试

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Rust 单元测试: AuthService (密码哈希, session, API key, TOTP) | **done** |
| T2 | Rust 单元测试: AlertService (阈值评估, 离线检测, 流量周期计算) | **done** |
| T3 | Rust 单元测试: NotificationService (模板变量替换, 渠道分发) | **done** |
| T4 | Rust 单元测试: RecordService (小时聚合, 清理保留逻辑) | **done** |
| T5 | Rust 集成测试: Agent 注册 → WS 连接 → 上报 → 指标入库 | **done** |
| T6 | Rust 集成测试: 备份恢复往返 (backup → restore → 验证数据完整) | **done** |
| T7 | 前端测试: Vitest 配置 + hooks 单元测试 (use-auth, use-api) | **done** |
| T8 | E2E 测试: Playwright 配置 + 登录/仪表盘/终端 流程验证 | **跳过** |
| T9 | CI 添加 `cargo test --workspace` 步骤 | **done** |

### P3-d: Agent 完善

| Task | 名称 | 状态 |
|------|------|------|
| T1 | 虚拟化检测 (读取 /sys/class/dmi 或 systemd-detect-virt) | **done** |
| T2 | Config enable_temperature/enable_gpu 开关生效 (当前 collector 无条件运行) | **done** (已存在) |
| T3 | Windows TCP/UDP 连接数采集 (当前仅 Linux /proc/net/tcp) | **done** |

### P3-e: 性能优化

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Vite Code Split (manualChunks: xterm.js, recharts 独立 chunk) | **done** |
| T2 | OpenAPI 类型生成 (openapi-typescript 替代手写接口定义) | **done** |
| T3 | 数据库查询优化 (大时间范围 records 查询索引 + 分页) | **done** (索引已存在) |

### P3-f: CI/CD + 部署

| Task | 名称 | 状态 |
|------|------|------|
| T1 | CI 添加 Windows Agent 构建 (x86_64-pc-windows-msvc target) | **done** |
| T2 | README.md 内容 (功能列表, 快速开始, 安装指南, 截图) | **done** |
| T3 | 部署文档: OAuth 配置 (GitHub/Google/OIDC credentials) | **done** (README 内) |
| T4 | 部署文档: 2FA/GeoIP MMDB/备份恢复 配置说明 | **done** (README 内) |
| T5 | Fumadocs 文档站内容 (16 页中英文双语文档) | **done** |

### P4: 端到端验证 + 上线前加固

| Task | 名称 | 状态 |
|------|------|------|
| T1 | 端到端集成验证 (server + agent 启动 → 注册 → WS → 48 个 API 验证) | **done** |
| T2 | 前后端接口字段一致性检查 (所有类型定义匹配) | **done** |
| T3 | 修复 Figment env split("_") → split("__") | **done** |
| T4 | 修复 Alert 表单 (添加 cover_type/server_ids 选择器) | **done** |
| T5 | 修复 Server Edit (添加 group_id 下拉选择器) | **done** |
| T6 | 前端路由守卫 (非 admin 用户 admin-only 页面重定向) | **done** |
| T7 | 默认密码提醒 (must_change_password API + banner + 启动 WARN) | **done** |

### P5: Agent Capability Toggles (Per-Agent 功能开关)

**设计文档**: `docs/superpowers/specs/2026-03-14-agent-capability-toggles-design.md`
**实现计划**: `docs/superpowers/plans/2026-03-14-agent-capability-toggles-plan.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Common: 能力位图常量 (CAP_TERMINAL=1..CAP_PING_HTTP=32, CAP_DEFAULT=56) | **done** |
| T2 | Common: 协议扩展 (CapabilitiesSync, CapabilityDenied, BrowserMessage 新变体) | **done** |
| T3 | Server: DB migration 添加 capabilities + protocol_version 列 | **done** |
| T4 | Server: AgentManager protocol_version 追踪 + broadcast_browser | **done** |
| T5 | Server: AppError::Forbidden(String) 变体 | **done** |
| T6 | Server: API 响应添加 capabilities + batch-capabilities 端点 | **done** |
| T7 | Server: Agent WS Welcome 发送实际 capabilities, 处理 CapabilityDenied | **done** |
| T8 | Server: Terminal WS CAP_TERMINAL 拦截 (403) | **done** |
| T9 | Server: Task API CAP_EXEC 过滤 + 合成 task_result (exit_code=-2) | **done** |
| T10 | Server: PingService 按 CAP_PING_* 过滤任务 | **done** |
| T11 | Server: update_server/batch 实时推送 CapabilitiesSync + Browser 广播 + 选择性 ping 重同步 | **done** |
| T12 | Agent: reporter.rs capabilities Arc\<AtomicU32\> + Welcome 解析 + CAP_EXEC/CAP_UPGRADE 校验 | **done** |
| T13 | Agent: PingManager 按 capability 过滤 incoming configs | **done** |
| T14 | Agent: TerminalManager CAP_TERMINAL 校验 | **done** |
| T15 | Integration: 集成测试修复 (protocol_version=2, ping_tasks_sync 消息顺序) | **done** |
| T16 | Frontend: 共享 capabilities 常量 + hasCap helper | **done** |
| T17 | Frontend: WS hook capabilities_changed + agent_info_updated 处理 | **done** |
| T18 | Frontend: Server Detail 页面 CapabilitiesSection + Terminal 按钮条件隐藏 | **done** |
| T19 | Frontend: Settings/Capabilities 管理页面 (批量 toggle + 搜索 + 多选) | **done** |
| T20 | Frontend: Tasks 页面灰显无 CAP_EXEC 服务器 + skipped 标记 | **done** |
| T21 | Server: OpenAPI 更新 (batch-capabilities 路径 + schema) | **done** |
| T22 | Common: constants.rs 单元测试 (6 个) | **done** |
| T23 | Common: protocol.rs 序列化测试 (5 个) | **done** |
| T24 | Frontend: capabilities.test.ts (3 个 vitest 测试) | **done** |
| T25 | 代码审查修复 (CapabilityDenied 写 task_result, audit log, selective ping re-sync, clippy) | **done** |

### P6: 文件管理 (File Management)

**分支**: `denver`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Common: FileEntry/FileType 类型定义 | **done** |
| T2 | Common: CAP_FILE 能力位 + 文件常量 | **done** |
| T3 | Common: 文件协议消息 (FileList/Stat/Read/Write/Delete/Mkdir/Move/Download/Upload) | **done** |
| T4 | Agent: FileConfig (enabled/root_paths/max_file_size/deny_patterns) | **done** |
| T5 | Agent: FileManager (路径安全校验 + 文件操作) | **done** |
| T6 | Agent: reporter.rs 集成 FileManager | **done** |
| T7 | Server: AgentManager 请求-响应中继 (pending_requests + oneshot) | **done** |
| T8 | Server: AppError::RequestTimeout 变体 | **done** |
| T9 | Server: FileTransferManager (并发控制 + 临时文件 + 过期清理) | **done** |
| T10 | Server: AppState 添加 file_transfers | **done** |
| T11 | Server: Agent WS handler 分发文件消息 | **done** |
| T12 | Server: File API Router (读/写分离, 13 个端点) | **done** |
| T13 | Server: 传输清理 + pending request 清理 | **done** |
| T14 | Frontend: CAP_FILE 能力常量 | **done** |
| T15 | Frontend: Server Detail 添加 Files 按钮 | **done** |
| T16 | Frontend: File API hooks (useFileList/Read/Write/Delete/Mkdir/Move/Download/Upload) | **done** |
| T17 | Frontend: Monaco Editor 组件封装 | **done** |
| T18 | Frontend: 文件浏览器组件 (FileBrowser/FileList/Breadcrumbs/FileEditor/FileUpload) | **done** |
| T19 | Frontend: 文件管理路由页面 (_authed/files.$serverId.tsx) | **done** |
| T20 | Frontend: i18n 中英文翻译 | **done** |
| T21 | 集成测试: 文件浏览 + 能力校验 + 传输端点 (3 tests) | **done** |
| T22 | 前端测试: file-utils 工具函数 (13 vitest tests) | **done** |
| T23 | 文档更新: TESTING.md + ENV.md + PROGRESS.md + Fumadocs | **done** |
| T24 | 最终验证: cargo test + clippy + typecheck + vitest + build | **done** |

### P7: Docker 容器监控 (Docker Monitoring)

**分支**: `feature/docker-monitoring`
**设计文档**: `docs/superpowers/specs/2026-03-18-docker-monitoring-design.md`
**实现计划**: `docs/superpowers/plans/2026-03-18-docker-monitoring.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Common: Docker 数据结构 (DockerContainer/Stats/Log/Event/SystemInfo/Network/Volume/Action) | **done** |
| T2 | Common: CAP_DOCKER=128 能力位 + SystemInfo features 字段 | **done** |
| T3 | Common: Docker 协议消息 (AgentMessage 10 变体 + ServerMessage 11 变体 + BrowserMessage 3 变体 + BrowserClientMessage) | **done** |
| T4 | Server: 数据库 migration (docker_event 表 + features 列) | **done** |
| T5 | Server: DockerEvent entity + DockerService (save/query/cleanup) | **done** |
| T6 | Server: DockerViewerTracker (viewer refcount, 5 单元测试) | **done** |
| T7 | Server: AgentManager 扩展 (Docker 缓存 + features + log session 路由) | **done** |
| T8 | Server: AppState async new() + docker_viewers 字段 | **done** |
| T9 | Server: Docker REST API (containers/stats/info/events/networks/volumes/action, 7 端点) | **done** |
| T10 | Server: Agent WS Docker 消息处理 (DockerInfo/Containers/Stats/Log/Event/Unavailable/Features) | **done** |
| T11 | Server: Browser WS DockerSubscribe/Unsubscribe + viewer 管理 | **done** |
| T12 | Server: Docker Log WebSocket 端点 (/ws/docker/logs/:server_id) | **done** |
| T13 | Server: CAP_DOCKER 撤销清理 + docker_event 定期清理 | **done** |
| T14 | Agent: DockerManager 核心 (bollard client + containers + stats polling) | **done** |
| T15 | Agent: Docker logs (批量发送) + events (自动重连) + networks + volumes | **done** |
| T16 | Agent: Reporter 集成 (DockerManager 初始化 + tokio::select! + features 上报) | **done** |
| T17 | Frontend: Docker 类型定义 + WsClient send/connectionState | **done** |
| T18 | Frontend: ServersWsContext + Docker WS 消息处理 (docker_update/event/availability) | **done** |
| T19 | Frontend: useDockerSubscription + useDockerLogs hooks | **done** |
| T20 | Frontend: Docker Tab 主页 + DockerOverview (5 卡片) + DockerEvents 时间线 | **done** |
| T21 | Frontend: ContainerList (表格 + 搜索 + 过滤) | **done** |
| T22 | Frontend: ContainerDetailDialog + ContainerStats (4 卡片) + ContainerLogs (实时流) | **done** |
| T23 | Frontend: DockerNetworksDialog + DockerVolumesDialog | **done** |
| T24 | Frontend: CAP_DOCKER 能力常量 + 服务器详情页 Docker 链接 + i18n | **done** |
| T25 | 集成测试 + E2E 验证 (23/24 浏览器测试通过) | **done** |

**E2E 中发现并修复的 Bug (4 个):**
1. `ServerResponse` DTO 缺少 `features` 字段 — API 无法返回 Docker 特性信息
2. Agent `poll_stats()` 只发 `DockerStats` 不发 `DockerContainers` — 导致 server 缓存空跳过广播
3. WS features 数据不能及时到达 React 组件 — 添加 REST API fallback
4. Docker 日志 WebSocket 协议不匹配 — 前端发送 `docker_logs_start` 但后端期望 `subscribe`，前端期望 `docker_log` 但后端发送 `logs`

### P8: 定时任务 (Scheduled Tasks)

**分支**: `feature/scheduled-tasks`
**实现计划**: `docs/superpowers/plans/2026-03-17-scheduled-tasks.md`
**设计文档**: `docs/superpowers/specs/2026-03-17-traffic-stats-scheduled-tasks-design.md` sections 2.1-2.6

| Task | 名称 | 状态 |
|------|------|------|
| T0 | 依赖: cron 0.13 crate | **done** |
| T1 | Server: pending_requests per-entry TTL (AgentManager + offline_checker) | **done** |
| T2 | Server: agent.rs TaskResult/CapabilityDenied pending dispatch | **done** |
| T3 | Server: TaskScheduler struct (overlap detection, cancellation, job管理) | **done** |
| T4 | Server: TaskScheduler job registration (add/remove/update/disable) | **done** |
| T5 | Server: 执行流程 (parallel per-server dispatch, retry, ActiveRunGuard) | **done** |
| T6 | Server: AppState 集成 + main.rs spawn scheduler | **done** |
| T7 | Server: Task API 扩展 (GET list, PUT update, DELETE, POST run) | **done** |
| T8 | Server: create_task 支持 scheduled 类型 + cron 验证 | **done** |
| T9 | Server: OpenAPI 注册新端点 + schema | **done** |
| T10 | Frontend: use-scheduled-tasks hooks (6 hooks, i18n toast) | **done** |
| T11 | Frontend: Tasks 页面 Tab 布局 (one-shot + scheduled) | **done** |
| T12 | Frontend: ScheduledTaskList 组件 (任务列表 + 执行历史) | **done** |
| T13 | Frontend: ScheduledTaskDialog 组件 (create/edit 表单) | **done** |
| T14 | Frontend: i18n 中英文翻译 (22 个 key) | **done** |
| T15 | 代码审查修复: 竞态条件 (DashMap entry API), run_task 验证, 时区感知 next_run_at | **done** |

**新增 API 端点:**
```
GET    /api/tasks                    列出任务 (?type=scheduled)
PUT    /api/tasks/:id               更新任务 (cron/enabled/command/servers/retry)
DELETE /api/tasks/:id               删除任务 (级联删除结果)
POST   /api/tasks/:id/run           手动触发定时任务 (409 if running)
```

**技术亮点:**
- DashMap `entry()` API 原子化重叠检测，消除竞态条件
- `ActiveRunGuard` drop guard 确保 `active_runs` 清理（即使 panic）
- per-entry TTL 支持定时任务 310s 超时（默认 60s 不变）
- Agent WS handler dispatch-before-save，scheduler 通过 oneshot channel 接收结果
- 时区感知的 `next_run_at` 计算 (chrono-tz)

**测试:** 5 个新单元测试 (TaskScheduler 3 + correlation_id 2), 所有 262 Rust + 124 前端测试通过

### P10: 流量统计 (Traffic Statistics)

**分支**: `main`
**Commits**: `846bd73`..`f28a696` (3 commits)

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Server: 流量总览 API (`GET /api/traffic/overview`) | **done** |
| T2 | Server: 全局日聚合 API (`GET /api/traffic/overview/daily?days=N`) | **done** |
| T3 | Server: 周期历史 API (`GET /api/traffic/{server_id}/cycle?history=N`) | **done** |
| T4 | Server: TrafficService 新增 `overview()`/`overview_daily()`/`cycle_history()` 方法 | **done** |
| T5 | Server: 新增类型 `ServerTrafficOverview`/`CycleTraffic`/`CycleResponse` + OpenAPI | **done** |
| T6 | Frontend: 全局流量总览页 (`/traffic`) — 统计卡片 + 服务器排名表格 + 30d 趋势 AreaChart | **done** |
| T7 | Frontend: 服务器详情 Traffic Tab — 周期进度条 + 日趋势 BarChart + 历史周期对比 BarChart | **done** |
| T8 | Frontend: 侧边栏 Traffic 导航入口 + i18n 中英文 key | **done** |
| T9 | Frontend: `traffic-card.test.tsx` (1 vitest) — TrafficCard tab 切换 + hourly/daily 图表渲染 | **done** |
| T10 | 最终验证: cargo test + clippy + typecheck + ultracite check 全部通过 | **done** |

**新增 API 端点:**
```
GET    /api/traffic/overview              所有服务器当前计费周期用量汇总
GET    /api/traffic/overview/daily        全局日聚合流量（?days=30）
GET    /api/traffic/{server_id}/cycle     指定服务器历史周期对比（?history=6）
```

**测试:** 1 个新前端测试 (traffic-card.test.tsx), 全部 298 Rust + 124 前端测试通过

### P9: 服务监控 (Service Monitor)

**分支**: `main`
**Commits**: `f02fd23`..`dcca19e` (10 commits)

| Task | 名称 | 状态 |
|------|------|------|
| T1 | 依赖: x509-parser, hickory-resolver, whois-rust | **done** |
| T2 | Server: 数据库 migration (service_monitor + service_monitor_record 表 + 索引) | **done** |
| T3 | Server: service_monitor + service_monitor_record sea-orm 实体 | **done** |
| T4 | Server: ServiceMonitorService (CRUD + record 管理 + check_state 更新 + 清理) | **done** |
| T5 | Server: 5 种 Checker 实现 (SSL 证书到期 / DNS 解析 / HTTP 关键词 / TCP 端口 / WHOIS 到期) | **done** |
| T6 | Server: 后台执行引擎 (ServiceMonitorTask — 并发 + 调度 + 重试 + 状态写入) | **done** |
| T7 | Server: 保留配置 + 定期清理 (service_monitor_record_days) | **done** |
| T8 | Server: REST API (7 端点: read_router + write_router + OpenAPI 注解) | **done** |
| T9 | Frontend: 服务监控管理页面 (列表 + 创建/编辑对话框 + 手动触发 + 侧边栏导航) | **done** |
| T10 | Frontend: 服务监控详情页面 (状态图表 + 历史记录表格 + 实时刷新) | **done** |
| T11 | 最终验证: cargo test + clippy + typecheck + ultracite check 全部通过 | **done** |

**新增 API 端点:**
```
GET    /api/service-monitors              列出所有服务监控
POST   /api/service-monitors             创建服务监控
GET    /api/service-monitors/:id         获取服务监控详情
PUT    /api/service-monitors/:id         更新服务监控
DELETE /api/service-monitors/:id         删除服务监控 (级联删除记录)
GET    /api/service-monitors/:id/records 获取检测记录 (?from=&to=)
POST   /api/service-monitors/:id/check   手动触发检测
```

**支持的监控类型:**
- **SSL** — 检测 TLS 证书到期天数 (x509-parser)
- **DNS** — 验证域名解析是否返回预期 IP (hickory-resolver)
- **HTTP Keyword** — HTTP/HTTPS 响应体关键词匹配 (reqwest)
- **TCP** — TCP 端口连通性检测
- **WHOIS** — 域名到期日检测 (whois-rust)

**测试:** 36 个新单元测试 (ServiceMonitorService 15 + checker 21: dns 3 + http_keyword 3 + ssl 3 + tcp 3 + whois 8), 所有 298 Rust + 124 前端测试通过

### P11: IP 变更通知 (IP Change Notification)

**分支**: `main`
**实现计划**: `docs/superpowers/plans/2026-03-19-p11-ip-change-notification.md`
**设计文档**: `docs/superpowers/specs/2026-03-19-batch1-batch2-features-design.md` Section 3

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Common: IpChanged AgentMessage + ServerIpChanged BrowserMessage + NetworkInterface 类型 | **done** |
| T2 | Server: migration 添加 last_remote_addr 列到 servers 表 | **done** |
| T3 | Server: AlertStateManager 提升到 AppState 共享访问 | **done** |
| T4 | Server: AlertService::check_event_rules() — 事件驱动告警派发 (ip_changed 规则类型) | **done** |
| T5 | Server: 被动检测 (Agent 连接时 remote_addr 对比) + IpChanged 消息处理 | **done** |
| T6 | Agent: IpChangeConfig + 5 分钟 NIC 枚举定时器 + 可选外部 IP 检查 | **done** |
| T7 | Frontend: alerts.tsx 新增 "IP Changed" 规则类型 (无阈值字段) | **done** |
| T8 | 最终验证: cargo test + clippy + typecheck + ultracite check + 文档更新 | **done** |

**新增协议消息:**
- `AgentMessage::IpChanged { ipv4, ipv6, interfaces }` — Agent 检测到 IP 变更时上报
- `BrowserMessage::ServerIpChanged { server_id, old_ipv4, new_ipv4, ... }` — 广播到浏览器客户端

**新增 Agent 配置:**
- `ip_change.enabled` (default `true`) — 是否启用 IP 变更检测
- `ip_change.check_external_ip` (default `false`) — 是否同时检查外部公网 IP
- `ip_change.external_ip_url` (default `https://api.ipify.org`) — 外部 IP 查询 URL
- `ip_change.interval_secs` (default `300`) — 检测间隔（秒）

**测试:** 0 个新专项测试 (现有 273 单元 + 26 集成 + 4 Docker 集成 + 124 前端 = 427 个测试通过)

### P12: 磁盘 I/O 监控 (Disk I/O Monitoring)

**分支**: `sydney-v1`
**提交**: `1a6d1da` `feat: add historical disk I/O monitoring`
**本地待提交修复**: ping OpenAPI `operation_id` 去重（`crates/server/src/router/api/ping.rs`、`apps/web/openapi.json`、`apps/web/src/lib/api-types.ts`）
**实现计划**: `docs/superpowers/plans/2026-03-19-p12-disk-io-monitoring.md`
**设计文档**: `docs/superpowers/specs/2026-03-19-batch1-batch2-features-design.md` Section 4

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Common: `DiskIo` 类型 + `SystemReport.disk_io` + 协议向后兼容测试 | **done** |
| T2 | Agent: Linux `/proc/diskstats` 采集、物理盘过滤、首样本基线语义 | **done** |
| T3 | Server: `records` / `records_hourly` 增加 `disk_io_json` migration + 实体字段 | **done** |
| T4 | Server: `RecordService::save_report()` 持久化 `disk_io_json` | **done** |
| T5 | Server: 小时聚合按磁盘名求平均并写入 `records_hourly.disk_io_json` | **done** |
| T6 | Server: `/api/servers/{id}/records` OpenAPI schema + frontend type generation | **done** |
| T7 | Frontend: `disk-io.ts` 解析/汇总工具 + `DiskIoChart` 组件 | **done** |
| T8 | Frontend: Server Detail 页面集成 Disk I/O 图表（历史模式） | **done** |
| T9 | Frontend: 修复 Server Detail range key 冲突 (`raw/hourly` 不再折叠) | **done** |
| T10 | 测试与文档: Rust/Vitest/TESTING.md/PROGRESS.md 更新 | **done** |

**新增/变更点:**
- Agent 新增 `collector/disk_io.rs`，Linux 平台读取 `/proc/diskstats`，非 Linux 使用 sysinfo `Disk::usage()` + mount_point key（P19 实现）
- 记录表使用 `disk_io_json` JSON 字段承载每块磁盘的读写速率，避免新增一对多表
- 服务器详情页新增 Disk I/O 历史图表，支持 Merged / Per Disk 两种视图
- 本期保持 historical-only，不扩展 WebSocket realtime `ServerStatus`

**验证进展 (2026-03-20):**
- 自动化通过：`cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo build --workspace`、`bun run test`、`bun run typecheck`、`bun x ultracite check`、`bun run build`
- 前端构建修复：为 ping API 显式设置唯一 `operation_id`（`list_ping_tasks` / `update_ping_task` / `delete_ping_task`），并重新生成 `apps/web/openapi.json` 与 `apps/web/src/lib/api-types.ts`
- agent-browser 手动 / E2E：DI1~DI10 全部通过；已验证 realtime 隐藏、historical 渲染、Merged/Per Disk、tooltip、缺失点补零、`0 B/s`、range 切换、`disk_io_json = null` 兼容、API JSON 解析与中文文案
- 证据：截图保存在 `/tmp/serverbee-p12-manual-20260319/artifacts`

### P19: 跨平台磁盘 I/O (Cross-Platform Disk I/O)

**分支**: `kabul-v1`
**Commits**: `a1996e1`..`a87ace7` (3 commits)
**设计文档**: `docs/superpowers/specs/2026-03-22-cross-platform-disk-io-design.md`
**实现计划**: `docs/superpowers/plans/2026-03-22-cross-platform-disk-io.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | 添加 `compute_disk_io` mount-path key 单元测试 | **done** |
| T2 | 实现 sysinfo fallback `read_disk_counters()` + 更新集成测试 | **done** |
| T3 | 更新 TESTING.md | **done** |

**新增/变更点:**
- `read_disk_counters()` 非 Linux 分支从 `None` → sysinfo `Disk::usage()` 实现
- 使用 `mount_point()` 作为 key（per-mount-path 语义，非 per-physical-disk）
- 使用 `DiskRefreshKind::nothing().with_io_usage()` 仅刷新 I/O 数据
- Linux `/proc/diskstats` 实现完全不变
- 已知限制：macOS APFS 可能 overcount、Windows 多挂载路径可能重复

### P17: 自定义仪表盘 (Custom Dashboard)

**分支**: `main`
**Commits**: `6b326d3`..`b26d8c9` (14 commits)

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Server: 数据库 migration (dashboard + dashboard_widget 表) | **done** |
| T2 | Server: DashboardService (CRUD + widget diff + default 管理 + 12 单元测试) | **done** |
| T3 | Server: AlertService::list_events() 聚合端点 + 2 单元测试 | **done** |
| T4 | Server: Dashboard REST API (6 端点: read_router + write_router + OpenAPI 注解) | **done** |
| T5 | Server: 集成测试 (dashboard CRUD/default/RBAC + alert-events, 4 tests) | **done** |
| T6 | Frontend: react-grid-layout + Widget 类型定义 + dashboard hooks | **done** |
| T7 | Frontend: 安全 Markdown→HTML 转换器 + XSS 防护测试 (8 tests) | **done** |
| T8 | Frontend: 简化版 SVG 世界地图数据 | **done** |
| T9 | Frontend: 实时 Widget (stat-number, server-cards, gauge, top-n) | **done** |
| T10 | Frontend: 历史 Widget (line-chart, multi-line, traffic-bar, disk-io) | **done** |
| T11 | Frontend: 状态 Widget (alert-list, service-status, server-map, markdown) | **done** |
| T12 | Frontend: Dashboard Grid + Widget Renderer + Picker + Config Dialog | **done** |
| T13 | Frontend: 首页重写为自定义仪表盘系统 + Dashboard Switcher | **done** |
| T14 | Frontend: 前端测试 (hooks 7 + widget-renderer 13 + grid 4 + config-dialog 8, 共 32 tests) | **done** |
| T15 | 最终验证: cargo test + clippy + typecheck + ultracite check + OpenAPI 重新生成 | **done** |
| T16 | 文档更新: PROGRESS.md + TESTING.md | **done** |

**新增 API 端点:**
```
GET    /api/dashboards              列出所有仪表盘
POST   /api/dashboards              创建仪表盘
GET    /api/dashboards/default      获取默认仪表盘（首次自动创建）
GET    /api/dashboards/:id          获取仪表盘详情（含 widgets）
PUT    /api/dashboards/:id          更新仪表盘（名称 + widget diff）
DELETE /api/dashboards/:id          删除仪表盘（禁止删除默认/最后一个）
GET    /api/alert-events            聚合告警事件 Feed（?limit=N）
```

**支持的 Widget 类型 (12 种):**
- **实时**: stat-number (单指标数值), server-cards (服务器卡片), gauge (仪表盘), top-n (排行榜)
- **历史**: line-chart (折线图), multi-line (多服务器对比), traffic-bar (流量柱图), disk-io (磁盘I/O)
- **状态**: alert-list (告警事件), service-status (服务监控), server-map (世界地图), markdown (自定义内容)

**测试:** 14 个新 Rust 单元测试 (DashboardService 12 + AlertService list_events 2) + 4 个新集成测试 + 40 个新前端 vitest 测试

### P13: 三网 Ping + Traceroute

**分支**: `feature/batch2-p13-p16`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Common: TracerouteHop 类型 + AgentMessage::TracerouteResult + ServerMessage::Traceroute | **done** |
| T2 | Agent: traceroute 执行器 (traceroute/mtr/tracert 命令, 输出解析, 目标验证, 能力校验) | **done** |
| T3 | Server: AgentManager traceroute_results DashMap 缓存 + 120s TTL 清理 | **done** |
| T4 | Server: Traceroute API (POST trigger + GET poll, 2 端点) | **done** |
| T5 | Frontend: 网络详情页 By Provider 分组视图 (CT/CU/CM/International) | **done** |
| T6 | Frontend: Traceroute UI (目标输入 + 结果表格 + 色彩编码延迟) | **done** |

**新增 API 端点:**
```
POST /api/servers/:id/traceroute        触发 traceroute
GET  /api/servers/:id/traceroute/:id    轮询结果
```

### P14: 多主题 + 品牌定制

**分支**: `feature/batch2-p13-p16`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Server: Brand API (GET/PUT config + POST logo/favicon 上传, 4 端点) | **done** |
| T2 | Server: 图片上传验证 (512KB 限制, PNG/ICO magic bytes) | **done** |
| T3 | Frontend: 7 主题 CSS (Tokyo Night/Nord/Catppuccin/Dracula/One Dark/Solarized/Rose Pine) | **done** |
| T4 | Frontend: ThemeProvider 扩展 (colorTheme + 动态 CSS 加载) | **done** |
| T5 | Frontend: Appearance 设置页 (主题选择网格 + 品牌设置) | **done** |

**新增 API 端点:**
```
GET  /api/settings/brand              获取品牌配置
PUT  /api/settings/brand              更新品牌配置
POST /api/settings/brand/logo         上传 Logo
POST /api/settings/brand/favicon      上传 Favicon
GET  /api/brand/logo                  获取 Logo 文件
GET  /api/brand/favicon               获取 Favicon 文件
```

### P15: 状态页增强

**分支**: `feature/batch2-p13-p16`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Server: Migration (status_page/incident/incident_update/maintenance/uptime_daily 5 表) | **done** |
| T2 | Server: 5 个 sea-orm 实体 | **done** |
| T3 | Server: StatusPageService/IncidentService/MaintenanceService/UptimeService 4 个服务 | **done** |
| T4 | Server: is_in_maintenance() 共享工具 + 告警/服务监控集成 | **done** |
| T5 | Server: 14+ API 端点 (status-pages/incidents/maintenances CRUD + 公开 status/:slug) | **done** |
| T6 | Server: Aggregator 扩展 (uptime_daily 每小时聚合) | **done** |
| T7 | Frontend: 公开状态页 /status/:slug (状态横幅 + 事件 + 维护 + 服务器 uptime) | **done** |
| T8 | Frontend: Admin 管理页 (3 Tab: 状态页/事件/维护 CRUD) | **done** |

**新增 API 端点:**
```
GET    /api/status/:slug               公开状态页数据 (无需认证)
GET    /api/status-pages               列出状态页
POST   /api/status-pages               创建状态页
PUT    /api/status-pages/:id           更新状态页
DELETE /api/status-pages/:id           删除状态页
GET    /api/incidents                  列出事件
POST   /api/incidents                  创建事件
PUT    /api/incidents/:id              更新事件
DELETE /api/incidents/:id              删除事件
POST   /api/incidents/:id/updates      添加事件更新
GET    /api/maintenances               列出维护窗口
POST   /api/maintenances               创建维护窗口
PUT    /api/maintenances/:id           更新维护窗口
DELETE /api/maintenances/:id           删除维护窗口
```

### P16: 移动端响应式 + PWA

**分支**: `feature/batch2-p13-p16`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | Frontend: Sidebar → Sheet 抽屉 (lg 以下 hamburger 菜单) | **done** |
| T2 | Frontend: Dashboard 网格响应式 (4→3→2→1 列) | **done** |
| T3 | Frontend: Dialog 小屏全屏化 | **done** |
| T4 | Frontend: PWA (vite-plugin-pwa + manifest + Service Worker) | **done** |
| T5 | Frontend: 服务器详情页响应式布局 | **done** |

---

## 下一步：上线前待办

### 必须做（上线前） — 全部完成 ✅

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| **P0** | 端到端集成验证 | **已完成** ✅ | 启动 server + agent 完整流程验证：注册→WS 连接→指标上报→全部 48 个 API 端点正常，无字段不匹配 |
| **P1** | 前端路由守卫 | **已完成** ✅ | 非 admin 用户访问 admin-only 页面时自动重定向到 Dashboard |
| **P2** | 首次部署默认密码 | **已完成** ✅ | Login/Me API 返回 `must_change_password`，前端 amber banner 提醒，启动日志 WARN |

### 验证中修复的 Bug (3 个)

1. **Figment env split** — `.split("_")` → `.split("__")`，修复环境变量含下划线字段名的解析问题 (server + agent)
2. **Alert 表单缺失字段** — 添加 `cover_type` 和 `server_ids` 选择器，支持选择规则覆盖范围
3. **Server 编辑缺失字段** — 添加 `group_id` 下拉选择器，支持修改服务器分组

### 建议做（提升质量） — 全部完成 ✅

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| **P3** | Rust 测试 (100 单元 + 10 集成) | **已完成** ✅ | 13 模块全覆盖 + 10 个端到端集成测试 |
| **P4** | 前端 Vitest 测试 (52 个) | **已完成** ✅ | 7 个测试文件：api-client/utils/ws-client/capabilities + hooks |

### 可以不做

- ~~P3-b T10 服务器列表页~~ — **已完成** (`1bea44d`)
- ~~P3-e T2 OpenAPI 类型生成~~ — **已完成**
- P3-c T8 Playwright E2E — 有单元测试和手动验证足够
- P3-f T5 Fumadocs — 有 README 和 Swagger UI 足够

## GeoIP 数据库一键下载

**分支**: `kabul-v1`
**设计文档**: `docs/superpowers/specs/2026-03-22-geoip-download-design.md`
**实现计划**: `docs/superpowers/plans/2026-03-22-geoip-download.md`

| Task | 名称 | 状态 |
|------|------|------|
| T1 | GeoIpService 重写 (RwLock hot-reload + download_dbip + 移除 ip-api.com) | **done** |
| T2 | agent.rs 调用链简化 (优先 reported IP + 脏数据清理) | **done** |
| T3 | GeoIP API 端点 (GET /geoip/status + POST /geoip/download) | **done** |
| T4 | Server Map widget 下载提示 | **done** |
| T5 | GeoIP Settings 页面 + 侧边栏导航 + i18n | **done** |
| T6 | Code review 修复 (路由拆分 read/write + query invalidation) | **done** |
| T7 | 单元测试 + 集成测试 + 前端测试 (14 tests) + TESTING.md 更新 | **done** |

### P20: 安全加固 Round 2 (Security Hardening Round 2)

**分支**: `chengdu`
**设计文档**: `docs/superpowers/specs/2026-03-28-security-hardening-round2-design.md`
**实现计划**: `docs/superpowers/plans/2026-03-28-security-hardening-round2.md`
**Commits**: `ae06e58`..`c25790f` (14 commits)

| Task | 名称 | 状态 |
|------|------|------|
| F1 | batch_update_capabilities 事务保护 (两阶段: DB txn + 副作用后置) | **done** |
| F2 | 安全响应头 (X-Frame-Options/X-Content-Type-Options/Referrer-Policy/X-Permitted-Cross-Domain-Policies) | **done** |
| F3 | 文件上传大小限制 (FileConfig.max_upload_size 100MB + 流式检查 + DefaultBodyLimit) | **done** |
| F4 | must_change_password 机制移除 (admin 密码已随机生成，默认密码检查不再需要) | **done** |
| F5 | fetch_external_ip 响应大小限制 (256 字节流式读取 + content_length 检查) | **done** |

**新增配置:**
- `SERVERBEE_FILE__MAX_UPLOAD_SIZE` — 文件上传大小限制，默认 100MB

**新增集成测试:** `test_security_headers_present` — 验证 4 个安全响应头存在
