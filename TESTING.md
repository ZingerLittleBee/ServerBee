# ServerBee 测试指南

## 快速命令

```bash
# 全量测试
cargo test --workspace && bun run test

# Rust 测试（100 单元 + 11 集成）
cargo test --workspace

# 前端测试（52 vitest）
bun run test

# 代码质量
cargo clippy --workspace -- -D warnings
bun x ultracite check
bun run typecheck
```

## Rust 测试

### 按 crate 运行

```bash
cargo test -p serverbee-common          # 协议 + 能力常量 (11 tests)
cargo test -p serverbee-server          # 服务端单元 + 集成 (93 tests)
cargo test -p serverbee-agent           # Agent 采集器 + Pinger (7 tests)
```

### 仅集成测试

```bash
cargo test -p serverbee-server --test integration
```

集成测试会启动真实 server + SQLite 临时数据库，无需外部依赖。

### 运行单个测试

```bash
cargo test -p serverbee-server test_hash_and_verify_password
cargo test --workspace -- --nocapture   # 显示 stdout
```

### 单元测试覆盖

| 模块 | 测试数 | 覆盖内容 |
|------|--------|----------|
| `common/constants.rs` | 6 | 能力位运算、默认值、掩码 |
| `common/protocol.rs` | 5 | 消息序列化/反序列化 |
| `server/service/alert.rs` | 15 | 阈值判定、指标提取、采样窗口 |
| `server/service/auth.rs` | 19 | 密码哈希、session、API key、TOTP、登录、改密 |
| `server/service/notification.rs` | 16 | 模板变量替换、渠道配置解析 |
| `server/service/record.rs` | 4 | 历史查询、聚合、清理策略 |
| `server/service/agent_manager.rs` | 10 | 连接管理、广播、缓存、终端会话、离线检测 |
| `server/service/server.rs` | 5 | 服务器 CRUD、批量删除 |
| `server/service/user.rs` | 4 | 用户 CRUD、级联删除、最后 admin 保护 |
| `server/service/ping.rs` | 3 | Ping 任务 CRUD |
| `server/middleware/auth.rs` | 6 | Cookie/API Key 提取 |
| `agent/collector/` | 5 | 系统信息、指标范围、使用量约束 |
| `agent/pinger.rs` | 2 | TCP 探测（开放/关闭端口） |

### 集成测试覆盖

| 测试 | 流程 |
|------|------|
| `test_agent_register_connect_report` | Agent 注册 → WS 连接 → SystemInfo → 指标上报 |
| `test_backup_restore` | 创建数据 → 备份 → 恢复 → 验证完整性 |
| `test_login_logout_flow` | 登录 → 检查状态 → 登出 → 验证 401 |
| `test_api_key_lifecycle` | 创建 API Key → 用 Key 访问 API |
| `test_member_read_only` | 创建 member → 读成功 → 写 403 |
| `test_public_status_no_auth` | 无认证访问 /status → 200 |
| `test_audit_log_recorded` | 登录 → 审计日志记录 login 操作 |
| `test_notification_and_alert_crud` | 通知 → 通知组 → 告警规则 CRUD |
| `test_user_management_crud` | 用户创建 → 列表 → 改角色 → 删除 |
| `test_settings_auto_discovery_key` | 获取 → 重新生成 → 验证不同 |
| `test_alert_states_endpoint` | 创建规则 → GET states 返回空 → 删除规则 |

## 前端测试

```bash
bun run test              # 单次运行（CI 用）
bun run test:watch        # 监听模式（开发用）

# 单个文件
cd apps/web && bunx vitest run src/lib/capabilities.test.ts
```

### 前端测试覆盖

| 文件 | 测试数 | 覆盖内容 |
|------|--------|----------|
| `capabilities.test.ts` | 3 | hasCap、toggle on/off、默认值 |
| `use-auth.test.tsx` | 4 | 登录/登出状态、fetch mock |
| `use-api.test.tsx` | 4 | server/records 数据获取、空 id 守卫 |
| `api-client.test.ts` | 6 | 数据解包、JSON 序列化、204、错误处理 |
| `utils.test.ts` | 21 | formatBytes/Speed/Uptime、countryCodeToFlag |
| `ws-client.test.ts` | 6 | URL 构造、handler 分发、重连、关闭 |
| `use-servers-ws.test.ts` | 8 | 数据合并、静态字段保护、在线状态切换 |

### 测试工具

- **vitest** — 测试框架（jsdom 环境）
- **@testing-library/react** — React 组件测试
- **@testing-library/jest-dom** — DOM 断言匹配器

## 代码质量检查

```bash
# Rust: clippy 0 warnings（CI 强制）
cargo clippy --workspace -- -D warnings

# 前端: Biome lint + format
bun x ultracite check      # 检查
bun x ultracite fix         # 自动修复

# TypeScript 类型检查（含 fumadocs）
bun run typecheck
```

## 手动功能验证（E2E）

### 启动本地环境

```bash
# 1. 构建前端（server 通过 rust-embed 嵌入 dist/）
cd apps/web && bun install && bun run build && cd ../..

# 2. 构建 Rust
cargo build --workspace

# 3. 启动 Server（设置管理员密码，开发环境关闭 secure cookie）
SB_ADMIN__PASSWORD=admin123 SB_AUTH__SECURE_COOKIE=false cargo run -p serverbee-server &

# 4. 获取 auto-discovery key（登录后调用 API）
curl -s -c /tmp/sb-cookies.txt -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin123"}'
curl -s -b /tmp/sb-cookies.txt http://localhost:9527/api/settings/auto-discovery-key
# 返回 {"data":{"key":"<discovery_key>"}}

# 5. 启动 Agent（server_url 是 HTTP 基础地址，不是 WS 路径）
SB_SERVER_URL="http://127.0.0.1:9527" SB_AUTO_DISCOVERY_KEY="<discovery_key>" cargo run -p serverbee-agent &

# Docker 方式
docker compose up -d
```

默认地址：`http://localhost:9527`，管理员用户名：`admin`

> **注意**：`SB_SERVER_URL` 应设置为 HTTP 基础地址（如 `http://127.0.0.1:9527`），Agent 会自动拼接 `/api/agent/register` 和 `/api/agent/ws?token=` 路径。不要传入 WebSocket 路径。

### 验证清单 — 页面渲染

| 功能 | 路由/地址 | 验证方法 | 状态 |
|------|-----------|----------|------|
| 登录 | `/login` | 输入 admin/密码登录，跳转 Dashboard | ✅ |
| Dashboard | `/` | 显示统计摘要卡片（Servers, Avg CPU, Memory, Bandwidth, Healthy），服务器卡片含实时指标 | ✅ |
| Servers 列表 | `/servers` | 表格显示服务器，支持搜索、排序、批量选择 | ✅ |
| 服务器详情 | `/servers/:id` | 系统信息（OS/CPU/RAM/Kernel）、7 个实时图表（CPU/Memory/Disk/Network In/Out/Load/Temperature）、时间范围切换（1h/6h/24h/7d/30d） | ✅ |
| Capability Toggles | `/servers/:id` (底部) | 6 个开关：Web Terminal/Remote Exec/Auto Upgrade (High Risk, 默认关) + ICMP/TCP/HTTP Probe (Low Risk, 默认开) | ✅ |
| 全局 Capabilities | `/settings/capabilities` | 表格视图管理所有服务器的能力开关，支持搜索和批量选择 | ✅ |
| Agent 连接 | Dashboard | Agent 自动注册获取 token → WebSocket 连接 → 指标上报 → Dashboard 显示 Online | ✅ |
| 用户管理 | `/settings/users` | 用户列表、添加/删除用户、角色显示 | ✅ |
| 通知 | `/settings/notifications` | Notification Channels + Notification Groups，各带 Add 按钮 | ✅ |
| 告警 | `/settings/alerts` | Alert Rules 列表 + Add 按钮 | ✅ |
| Ping 探测 | `/settings/ping-tasks` | Probe Tasks 列表 + Add 按钮 | ✅ |
| API Keys | `/settings/api-keys` | 创建表单（key name + Create）+ Active Keys 列表 | ✅ |
| Security | `/settings/security` | 2FA 设置（Set Up 2FA）+ 密码修改表单 | ✅ |
| 审计日志 | `/settings/audit-logs` | 表格显示操作记录（Time/Action/User/IP/Detail） | ✅ |
| 公共状态页 | `/status` | 无需登录，显示服务器在线状态和实时指标 | ✅ |
| Swagger UI | `/swagger-ui/` | OpenAPI 文档加载正常 | — |
| 终端 | `/terminal/:serverId` | 需启用 Web Terminal capability 后测试 | — |
| 备份恢复 | Settings → Backup | 创建数据 → 备份 → 恢复 → 验证完整性 | — |

### 验证清单 — E2E 用户操作（agent-browser）

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| 1a | 错误密码提示 | 输入错误密码 → 显示 "Unauthorized" 错误文本 | ✅ |
| 1b | 正确登录跳转 | 输入正确密码 → 跳转到 `/` (Dashboard) | ✅ |
| 1c | 登出回到登录页 | 点击 Log out → 跳转到 `/login` | ✅ |
| 2 | Dashboard 实时更新 | 等待 5s → CPU/Memory/Network 数值发生变化 (WebSocket 推送) | ✅ |
| 3a | 搜索匹配 | 输入 "New" → 表格显示匹配的服务器 | ✅ |
| 3b | 搜索无匹配 | 输入不存在的名称 → 表格为空 | ✅ |
| 3c | 编辑对话框 | 点击 Edit → 弹出对话框含 BASIC + BILLING 字段 | ✅ |
| 4 | 时间范围切换 | 点击 6h → 图表更新时间轴和数据 | ✅ |
| 5a | 创建用户 | Add User → 填写 username/password → Create → 列表出现新用户 | ✅ |
| 5b | 删除用户 | 删除 testuser → 列表仅剩 admin | ✅ |
| 6 | 通知渠道展示 | 创建 Webhook 通知渠道 → 列表显示名称和类型 | ✅ |
| 7 | API Key 展示 | 创建 API Key → Active Keys 显示 prefix 和创建日期 | ✅ |
| 8a | Capabilities 展示 | 页面显示 6 个开关，默认 3 关 3 开 | ✅ |
| 8b | 公共状态页 | 无需登录访问 `/status` → 显示服务器卡片和指标 | ✅ |
| 8c | 主题切换 | 点击 Toggle theme → 深色/浅色模式正确渲染 | ✅ |

### 验证清单 — 告警 & 通知全链路

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| A1 | 通知渠道创建 | 创建 Webhook + Telegram → 列表显示 2 个渠道 | ✅ |
| A2 | 通知组创建 | 创建 "E2E Test Group" 关联 2 个渠道 → 列表显示 "2 channel(s)" | ✅ |
| A3 | 测试通知发送 | 点击测试按钮 → Webhook (webhook.site) + Telegram 均收到消息 | ✅ |
| A4 | 阈值告警触发 | 创建 cpu ≥ 1% 规则 → 60s 后触发 → Webhook + Telegram 收到告警通知 | ✅ |
| A5 | 告警状态展示 | 点击 States → 显示 "New Server" 🔴 Triggered (2x) + 时间戳 | ✅ |
| A6 | 告警条件格式 | 规则摘要正确显示 `cpu ≥ 1 | always` 和 `offline 30s | once` | ✅ |
| A7 | 离线告警触发 | 创建 offline 30s 规则 → 停 Agent → 等待触发 | ⚠️ 未触发（需排查评估逻辑） |

### E2E 测试中发现并修复的 Bug

| Bug | 描述 | 修复 |
|-----|------|------|
| 登录错误消息 | 显示原始 JSON `{"error":{"code":"UNAUTHORIZED",...}}` | 解析 JSON 提取 `error.message` 字段 (`69af3e7`) |
| 通知表单明文密码 | password/bot_token/device_key 使用 `type="text"` | 改为 `type="password"` 掩码 (`82dcf15`) |
| 告警表单缺失字段 | 仅 12 种规则类型 + 仅 `max` 字段 | 扩展到 19 种 + 条件 min/duration/cycle 字段 |
| 告警状态无 UI | 后端有 alert_state 但前端无展示 | 新增 API + 可展开 per-server 状态面板 (`a8defea`) |

## 测试文件位置

```
crates/common/src/constants.rs          # 能力常量测试
crates/common/src/protocol.rs           # 协议序列化测试
crates/server/src/service/alert.rs      # 告警服务测试
crates/server/src/service/auth.rs       # 认证服务测试 (含 DB 集成)
crates/server/src/service/notification.rs # 通知服务测试
crates/server/src/service/record.rs     # 记录服务测试
crates/server/src/service/agent_manager.rs # AgentManager 单元测试
crates/server/src/service/server.rs     # 服务器 CRUD 测试
crates/server/src/service/user.rs       # 用户服务测试
crates/server/src/service/ping.rs       # Ping 服务测试
crates/server/src/middleware/auth.rs    # 中间件 Cookie/Key 提取测试
crates/server/src/test_utils.rs         # 测试辅助 (setup_test_db)
crates/server/tests/integration.rs      # 集成测试 (11 tests)
crates/agent/src/collector/tests.rs     # Agent 采集器测试
crates/agent/src/pinger.rs              # Agent Pinger 测试
apps/web/src/lib/capabilities.test.ts   # 能力位测试
apps/web/src/lib/api-client.test.ts     # API Client 测试
apps/web/src/lib/utils.test.ts          # 工具函数测试
apps/web/src/lib/ws-client.test.ts      # WebSocket Client 测试
apps/web/src/hooks/use-auth.test.tsx    # Auth hook 测试
apps/web/src/hooks/use-api.test.tsx     # API hook 测试
apps/web/src/hooks/use-servers-ws.test.ts # WS 数据合并测试
apps/web/vitest.config.ts               # Vitest 配置
.github/workflows/ci.yml               # CI 流水线
```
