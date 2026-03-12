# ServerBee 实现进度

> 最后更新: 2026-03-12

## 总览

| Plan | 名称 | 状态 | Commits |
|------|------|------|---------|
| Plan 1 | Foundation (Workspace + DB + Auth + API) | **已完成** | 6 |
| Plan 2 | Agent (采集 + 上报 + 注册) | **已完成** | 1 |
| Plan 3 | Real-time (WS + 后台任务) | **已完成** | 1 |
| Plan 4 | Frontend (路由 + 仪表盘 + 详情) | **已完成** | 2 |

**P0 MVP 代码已全部完成并通过端到端验证。** Server + Agent + Frontend 完整数据流已跑通：注册→WS连接→实时上报→API查询→浏览器WS推送。

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
| T17 | OpenAPI Documentation | **跳过 (P1)** |
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

---

## 已实现的文件清单

### Rust (66 files)

**crates/common/src/** (4 files)
- `lib.rs`, `types.rs`, `constants.rs`, `protocol.rs`

**crates/server/src/** (50 files)
- `main.rs`, `config.rs`, `state.rs`, `error.rs`
- `entity/` (20 files): user, session, api_key, server, server_group, server_tag, record, record_hourly, gpu_record, config, alert_rule, alert_state, notification, notification_group, ping_task, ping_record, task, task_result, audit_log
- `migration/` (2 files): mod.rs, m20260312_000001_init.rs
- `middleware/` (2 files): mod.rs, auth.rs
- `service/` (6 files): mod.rs, auth.rs, server.rs, config.rs, record.rs, agent_manager.rs
- `router/api/` (6 files): mod.rs, auth.rs, server.rs, server_group.rs, setting.rs, agent.rs
- `router/ws/` (3 files): mod.rs, agent.rs, browser.rs
- `router/` (1 file): mod.rs
- `task/` (6 files): mod.rs, record_writer.rs, aggregator.rs, cleanup.rs, offline_checker.rs, session_cleaner.rs

**crates/agent/src/** (12 files)
- `main.rs`, `config.rs`, `register.rs`, `reporter.rs`
- `collector/` (8 files): mod.rs, cpu.rs, memory.rs, disk.rs, network.rs, load.rs, process.rs, temperature.rs

### Frontend (24 files)

**apps/web/src/**
- `main.tsx`, `router.tsx`, `routeTree.gen.ts`
- `lib/`: api-client.ts, ws-client.ts, utils.ts
- `hooks/`: use-auth.ts, use-servers-ws.ts, use-api.ts
- `components/ui/`: button.tsx
- `components/layout/`: sidebar.tsx, header.tsx, theme-toggle.tsx
- `components/server/`: server-card.tsx, status-badge.tsx, metrics-chart.tsx
- `components/`: theme-provider.tsx
- `routes/`: __root.tsx, login.tsx, _authed.tsx, index.tsx (redirect)
- `routes/_authed/`: index.tsx (dashboard), servers/$id.tsx (detail)
- `routes/_authed/settings/`: index.tsx, api-keys.tsx

---

## 未完成的工作

### 待验证 (高优先级)
- [x] 端到端集成测试: 启动 server + agent, 验证注册→连接→上报→仪表盘展示完整流程 ✅
- [x] 修复集成中发现的 bug ✅ (见下方 bug 修复清单)
- [x] 清理编译警告 (24 个 dead_code warnings → 0) ✅

#### 已修复的集成 Bug
1. **Server Config `Default` 导致 panic**: `DatabaseConfig::default()` 的 `max_connections=0` 导致 SQLx pool panic。修复：所有 config struct 手动实现 `Default` 使用正确的默认值。
2. **API 泄露 token_hash**: `/api/servers` 返回了 `token_hash` 和 `token_prefix`。修复：新增 `ServerResponse` DTO 过滤敏感字段。
3. **前端 API 客户端未解包 `{ data: T }`**: `api-client.ts` 返回了整个 `ApiResponse` 而非内部 `data`。修复：自动提取 `.data`。
4. **前端 Auth 字段名错误**: User 接口用 `id` 而服务端返回 `user_id`，缺少 `role`。修复。
5. **WebSocket 消息格式不匹配**: `update` 用 singular `server`（应为 `servers` 数组），`server_online/offline` 期望完整对象（实际只有 `server_id`）。修复。
6. **ServerMetrics 字段名全错**: `cpu_usage→cpu`, `memory_total→mem_total`, `network_in_speed→net_in_speed`, `load_avg→load1/5/15` 等。修复。
7. **API 路径错误**: Settings 页面用 `/api/settings/discovery`（应为 `/api/settings/auto-discovery-key`），API Keys 用 `/api/settings/api-keys`（应为 `/api/auth/api-keys`）。修复。
8. **ServerRecord 字段名错误**: `cpu_usage→cpu`, `timestamp→time`, `memory_used→mem_used` 等。修复。
9. **Server Detail 页面时间范围**: interval 参数 `1m/5m/15m/1h/6h` 改为 `raw/hourly` 匹配服务端。

### 待实现: P1 功能
- [ ] 告警规则引擎 (资源阈值 + 流量周期 + 离线检测)
- [ ] 通知系统 (Webhook, Telegram, Email, Bark)
- [ ] Ping 探测任务 (ICMP/TCP/HTTP)
- [ ] 远程命令执行 (下发 + 结果)
- [ ] Web 终端 (PTY 代理)
- [ ] 温度/GPU 采集 (Agent 端)
- [ ] GeoIP 查询
- [ ] OAuth (GitHub, Google, OIDC)
- [ ] 2FA (TOTP)
- [ ] OpenAPI 文档 (utoipa + Swagger UI)
- [ ] 告警管理前端页面
- [ ] 通知配置前端页面
- [ ] Ping 任务前端页面
- [ ] Web 终端前端页面

### 待实现: P2 功能
- [ ] 多用户 (Admin/Member 角色)
- [ ] 审计日志
- [ ] 备份恢复
- [ ] Agent 自动更新
- [ ] 公开状态页
- [ ] 计费信息管理

### 待实现: 部署
- [x] Dockerfile (多阶段构建: bun build → cargo build → alpine) ✅
- [x] docker-compose.yml ✅
- [x] install.sh (安装脚本) ✅
- [x] systemd service units ✅
- [ ] GitHub Actions CI/CD
- [x] rust-embed 嵌入前端到 server 二进制 ✅

### 代码质量
- [ ] 单元测试
- [ ] 代码审查 (spec compliance + quality)
- [x] `bun x ultracite fix` 格式化前端代码 ✅

---

## Git Commits (实现部分)

```
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

## 下次继续的建议

1. **先跑通**: `cargo run -p serverbee-server` 启动服务端，创建 `agent.toml`，`cargo run -p serverbee-agent` 启动 Agent，验证完整数据流
2. **修 bug**: 端到端测试中必然会发现一些集成问题，逐个修复
3. **P1 功能**: 按优先级推进告警/通知/终端等
4. **部署**: 写 Dockerfile + install.sh，让项目可以实际部署
