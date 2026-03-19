# ServerBee 测试指南

## 快速命令

```bash
# 全量测试
cargo test --workspace && bun run test

# Rust 测试（268 单元 + 30 集成 = 298）
cargo test --workspace

# 前端测试（124 vitest，14 个测试文件）
bun run test

# 代码质量
cargo clippy --workspace -- -D warnings
bun x ultracite check
bun run typecheck
```

## Rust 测试

### 按 crate 运行

```bash
cargo test -p serverbee-common          # 协议 + 能力常量 + Docker 类型 (35 tests)
cargo test -p serverbee-server          # 服务端单元 + 集成 (189 + 30 = 219 tests)
cargo test -p serverbee-agent           # Agent 采集器 + Pinger + NetworkProber + FileManager (44 tests)
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
| `common/constants.rs` | 8 | 能力位运算、默认值、掩码、CAP_FILE、CAP_DOCKER |
| `common/protocol.rs` | 21 | 消息序列化/反序列化（NetworkProbe + 文件管理 + Docker 全协议覆盖） |
| `common/docker_types.rs` | 3 | Docker 容器/动作/日志条目序列化/反序列化 |
| `common/types.rs` | 2 | SystemInfo features 字段默认值和序列化 |
| `server/service/alert.rs` | 15 | 阈值判定、指标提取、采样窗口 |
| `server/service/auth.rs` | 19 | 密码哈希、session、API key、TOTP、登录、改密 |
| `server/service/notification.rs` | 16 | 模板变量替换、渠道配置解析 |
| `server/service/record.rs` | 6 | 历史查询、聚合、清理策略、保存上报、过期清理 |
| `server/service/agent_manager.rs` | 13 | 连接管理、广播、缓存、终端会话、离线检测、请求-响应中继、per-entry TTL |
| `server/service/docker_viewer.rs` | 5 | 首位/末位观察者检测、has_viewers、批量连接移除、批量服务器移除 |
| `server/service/server.rs` | 5 | 服务器 CRUD、批量删除 |
| `server/service/user.rs` | 4 | 用户 CRUD、级联删除、最后 admin 保护 |
| `server/service/ping.rs` | 3 | Ping 任务 CRUD |
| `server/middleware/auth.rs` | 6 | Cookie/API Key 提取 |
| `agent/collector/` | 5 | 系统信息、指标范围、使用量约束 |
| `agent/pinger.rs` | 2 | TCP 探测（开放/关闭端口） |
| `server/service/audit.rs` | 3 | 审计日志记录、列表、排序 |
| `server/service/config.rs` | 5 | KV 存取、upsert、类型化读写 |
| `server/presets/mod.rs` | 8 | 预设目标加载、ID 唯一性、查找、分组元数据、探测类型校验 |
| `server/service/network_probe.rs` | 13 | 网络探测目标 CRUD、预设目标保护（update/delete 403）、default_target_ids 校验、server targets 分配+校验、探测记录查询 |
| `agent/probe_utils.rs` | 2 | 批量探测结果解析、地址解析 |
| `agent/network_prober.rs` | 2 | 网络探测任务调度、结果上报 |
| `server/service/file_transfer.rs` | 9 | 传输创建/获取、并发限制、过期清理、状态转换、进度更新、临时文件清理 |
| `agent/file_manager.rs` | 24 | 路径校验(root_paths/遍历/deny_patterns/多根/空根)、目录列表(排序/空目录/元数据)、文件读写(base64编解码/大小限制)、删除/创建目录/重命名、上传流程、下载分片 |
| `server/service/traffic.rs` | 17 | 增量计算(正常/重启/单方向重启/零值)、计费周期范围(月/季/年/自定义起始日)、预测算法(正常/早期/无限额)、DB 操作(upsert 累加/状态缓存/日聚合时区) |
| `server/service/task_scheduler.rs` | 3 | TaskScheduler 创建、重叠检测、取消活跃运行 |
| `server/task/task_scheduler.rs` | 2 | correlation_id 格式、唯一性 |
| `server/config.rs` | 1 | 时区解析（chrono-tz 验证） |
| `server/service/service_monitor.rs` | 15 | CRUD、记录管理、check_state 更新、清理策略、级联删除、JSON server_ids |
| `server/service/checker/dns.rs` | 3 | 自定义 nameserver、无效 nameserver、系统默认 resolver 构建 |
| `server/service/checker/http_keyword.rs` | 3 | 空 headers、无效 header 值、带自定义 headers 构建 |
| `server/service/checker/ssl.rs` | 3 | 默认端口解析、显式端口解析、config 覆盖端口解析 |
| `server/service/checker/tcp.rs` | 3 | 连接拒绝、连接成功（本机监听）、默认超时值 |
| `server/service/checker/whois.rs` | 8 | 多种日期格式解析、paid-till 格式、ISO 格式、注册商解析、长文本截断 |

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
| `test_network_probe_target_crud` | 创建目标 → 列表 → 更新 → 删除 |
| `test_network_probe_setting_crud` | 读取默认配置 → 更新间隔 → 验证持久化 |
| `test_network_probe_server_targets` | 获取 server 关联目标列表 → 验证预设目标存在 |
| `test_network_probe_builtin_protection` | 删除预设目标 → 返回 403 |
| `test_preset_target_source_field` | 验证预设 source/source_name 字段正确、自建目标 source 为 null |
| `test_preset_target_cannot_be_updated` | PUT 预设目标 → 返回 403 |
| `test_file_list_server_offline` | 启用 CAP_FILE → POST /files/list → 离线返回 404 |
| `test_file_capability_enforcement` | CAP_DEFAULT (无 CAP_FILE) → POST /files/list → 403 |
| `test_file_transfers_endpoint` | GET /files/transfers → 空列表 → DELETE 不存在 → 404 |
| `test_file_write_requires_admin` | member 用户 POST /files/write → 403 |
| `test_file_delete_requires_admin` | member 用户 POST /files/delete → 403 |
| `test_file_mkdir_requires_admin` | member 用户 POST /files/mkdir → 403 |
| `test_oneshot_task_backward_compat` | 新 migration 后一次性任务仍可正常创建 |
| `test_traffic_api_returns_data` | 注册 Agent → 查询流量 API → 验证响应结构 |
| `test_server_billing_start_day` | 更新 billing_start_day → 验证持久化和流量 API 反映 |

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
| `use-api.test.tsx` | 5 | server/records 数据获取、空 id 守卫、enabled 选项 |
| `api-client.test.ts` | 6 | 数据解包、JSON 序列化、204、错误处理 |
| `utils.test.ts` | 21 | formatBytes/Speed/Uptime、countryCodeToFlag |
| `ws-client.test.ts` | 6 | URL 构造、handler 分发、重连、关闭 |
| `use-realtime-metrics.test.tsx` | 13 | toRealtimeDataPoint 转换（百分比、零值除法、字段映射）、hook 集成（缓存 seed、去重、追加、裁剪、serverId 切换） |
| `use-servers-ws.test.ts` | 8 | 数据合并、静态字段保护、在线状态切换 |
| `use-terminal-ws.test.ts` | 20 | WS URL 构造、状态机、base64 编码、resize、onData 回调 |
| `file-utils.test.ts` | 30 | 扩展名→语言映射(yaml/json/ts/sh/rs/py/toml/go/sql/css/html/dockerfile/路径含点/大写)、文本文件判定(toml/sql/conf/exe/tar.gz)、图片文件判定(webp/ico/gif/bmp) |
| `use-traffic.test.tsx` | 3 | 流量数据获取、查询 key 验证、空 serverId 禁用 |

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

## i18n 验证

i18n 使用 react-i18next，覆盖全站 ~250 条字符串，支持中文/英文切换。

### 自动化验证

```bash
# TypeScript 类型检查（翻译 key 类型安全，缺失 key 会报错）
bun run typecheck

# 现有测试不依赖 UI 文本，应全部通过
bun run test
```

### 手动验证清单

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| i18n-1 | 浏览器语言自动检测 | 浏览器语言设为中文 → 打开页面 → UI 显示中文 | — |
| i18n-2 | 手动切换到中文 | 点击 Header 中的 "中文" 按钮 → 全站 UI 切换为中文 | — |
| i18n-3 | 手动切换到英文 | 点击 Header 中的 "EN" 按钮 → 全站 UI 切换为英文 | — |
| i18n-4 | 语言偏好持久化 | 切换语言 → 刷新页面 → 语言保持不变（localStorage） | — |
| i18n-5 | Dashboard 中文 | 中文模式下 Dashboard 显示"仪表盘"、"平均 CPU"、"平均内存"等 | — |
| i18n-6 | 服务器列表中文 | 中文模式下表头显示"名称"、"状态"、"内存"、"磁盘"等 | — |
| i18n-7 | 服务器详情中文 | 中文模式下图表标题显示"CPU 使用率"、"内存使用率"、时间范围显示"实时"、"1 小时"等 | — |
| i18n-8 | 设置页中文 | 中文模式下设置各子页面标题和表单标签全部显示中文 | — |
| i18n-9 | 登录页中文 | 中文模式下显示"登录 ServerBee"、"用户名"、"密码"等 | — |
| i18n-10 | 状态页语言切换 | 公开状态页（无需登录）也有语言切换按钮且功能正常 | — |
| i18n-11 | 侧边栏中文 | 中文模式下导航显示"仪表盘"、"服务器"、"用户管理"、"告警"等 | — |
| i18n-12 | 插值参数 | 中文模式下 "3 台服务器中 2 台在线"、"已选 5 项" 等带参数文本正确渲染 | — |
| i18n-13 | 品牌名保持 | 切换语言后 "ServerBee" 品牌名不被翻译 | — |
| i18n-14 | locale 变体检测 | 浏览器语言为 zh-CN / zh-TW → 正确回退到 zh | — |

## 手动功能验证（E2E）

### 启动本地环境

```bash
# 1. 构建前端（server 通过 rust-embed 嵌入 dist/）
cd apps/web && bun install && bun run build && cd ../..

# 2. 构建 Rust
cargo build --workspace

# 3. 启动 Server（设置管理员密码，开发环境关闭 secure cookie）
SERVERBEE_ADMIN__PASSWORD=admin123 SERVERBEE_AUTH__SECURE_COOKIE=false cargo run -p serverbee-server &

# 4. 获取 auto-discovery key（登录后调用 API）
curl -s -c /tmp/sb-cookies.txt -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin123"}'
curl -s -b /tmp/sb-cookies.txt http://localhost:9527/api/settings/auto-discovery-key
# 返回 {"data":{"key":"<discovery_key>"}}

# 5. 启动 Agent（server_url 是 HTTP 基础地址，不是 WS 路径）
SERVERBEE_SERVER_URL="http://127.0.0.1:9527" SERVERBEE_AUTO_DISCOVERY_KEY="<discovery_key>" cargo run -p serverbee-agent &

# Docker 方式
docker compose up -d
```

默认地址：`http://localhost:9527`，管理员用户名：`admin`

> **注意**：`SERVERBEE_SERVER_URL` 应设置为 HTTP 基础地址（如 `http://127.0.0.1:9527`），Agent 会自动拼接 `/api/agent/register` 和 `/api/agent/ws?token=` 路径。不要传入 WebSocket 路径。

### 验证清单 — 页面渲染

| 功能 | 路由/地址 | 验证方法 | 状态 |
|------|-----------|----------|------|
| 登录 | `/login` | 输入 admin/密码登录，跳转 Dashboard | ✅ |
| Dashboard | `/` | 显示统计摘要卡片（Servers, Avg CPU, Memory, Bandwidth, Healthy），服务器卡片含实时指标 | ✅ |
| Servers 列表 | `/servers` | 表格显示服务器，支持搜索、排序、批量选择 | ✅ |
| 服务器详情 | `/servers/:id` | 系统信息（OS/CPU/RAM/Kernel）、实时流式图表（默认）+ 历史图表（1h/6h/24h/7d/30d）、CPU/Memory/Disk/Network In/Out/Load/Temperature | ✅ |
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
| 远程命令（即时） | `/settings/tasks` (One-shot tab) | 命令输入 + 服务器选择 + 执行 + 结果展示 | ✅ |
| 定时任务（计划） | `/settings/tasks` (Scheduled tab) | 任务列表 + 创建/编辑/删除/暂停/手动执行 + 执行历史 | — |
| 服务监控 | `/settings/service-monitors` | 监控列表 + 创建/编辑/删除/手动触发 + 状态徽章 + 侧边栏导航 | — |
| 服务监控详情 | `/settings/service-monitors/:id` | 状态图表 + 历史记录表格 + 时间范围过滤 | — |
| 网络质量总览 | `/network` | 显示 VPS 网络质量卡片列表，统计栏显示总数/在线/异常 | — |
| 网络质量详情 | `/network/:id` | 目标卡片 + 多线延迟图表 + 异常摘要 + 底部统计 + CSV 导出 | — |
| 网络探测设置 | `/settings/network-probes` | 目标管理（96 预设 + 自定义 CRUD）+ 全局设置（间隔/包数/默认目标） | — |
| Docker 监控 | `/servers/:serverId/docker` | 概览卡片 + 容器表格 + 事件时间线 + 详情弹窗 + 网络/卷弹窗 | — |
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
| 4a | 实时模式默认 | 进入 `/servers/:id` → "Real-time" 按钮高亮选中（默认模式） | ✅ |
| 4b | 实时图表更新 | 实时模式下等待 10s → CPU/Memory/Disk/Network/Load 图表出现多个数据点，X 轴显示 mm:ss 格式 | ✅ |
| 4c | 实时首点时间格式 | X 轴第一个 tick 显示 HH:mm:ss（含小时），后续 tick 显示 mm:ss | ✅ |
| 4d | 实时→历史切换 | 点击 1h → 图表切换为历史数据，X 轴切换为 HH:mm 格式 | ✅ |
| 4e | 历史→实时切换 | 从 1h 切回 Real-time → 图表显示累积的实时数据（非空，之前已积累的点保留） | ✅ |
| 4f | 实时模式隐藏温度/GPU | 实时模式下 Temperature 和 GPU 图表不可见（WS 数据不含温度和 GPU） | ✅ |
| 4g | 历史模式显示温度 | 切换到 1h → 若有温度数据，Temperature 图表可见 | — |
| 4h | 离线服务器实时模式 | 离线服务器进入详情页 → 实时模式默认选中，图表为空（可接受） | — |
| 4i | 时间范围切换 | 点击 6h → 图表更新时间轴和数据 | ✅ |
| 5a | 创建用户 | Add User → 填写 username/password → Create → 列表出现新用户 | ✅ |
| 5b | 删除用户 | 删除 testuser → 列表仅剩 admin | ✅ |
| 6 | 通知渠道展示 | 创建 Webhook 通知渠道 → 列表显示名称和类型 | ✅ |
| 7 | API Key 展示 | 创建 API Key → Active Keys 显示 prefix 和创建日期 | ✅ |
| 8a | Capabilities 展示 | 页面显示 6 个开关，默认 3 关 3 开 | ✅ |
| 8b | 公共状态页 | 无需登录访问 `/status` → 显示服务器卡片和指标 | ✅ |
| 8c | 主题切换 | 点击 Toggle theme → 深色/浅色模式正确渲染 | ✅ |

### 验证清单 — 网络质量监控

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| N1 | 总览页渲染 | 登录后点击侧边栏「网络质量」→ `/network` 显示 VPS 卡片列表 | — |
| N2 | 预设目标展示 | 打开 `/settings/network-probes` → Tab 1 目标管理 → 表格显示 96 个预设目标（带锁图标和来源标签） | — |
| N3 | 自定义目标 CRUD | 点击添加目标 → 填写名称/运营商/地区/地址/类型 → 创建 → 列表 97 条 → 编辑 → 删除 → 回到 96 条 | — |
| N4 | 全局设置持久化 | Tab 2 全局设置 → 改间隔为 120s → 保存 → 刷新页面 → 间隔仍为 120s | — |
| N5 | 默认目标配置 | Tab 2 → 勾选 3 个默认目标 → 保存 → 新注册 Agent → 该 server 自动分配 3 个目标 | — |
| N6 | 服务器目标配置 | 详情页 → 管理目标 → 选择 4 个目标 → 保存 → 目标卡片显示 4 个 | — |
| N7 | 详情页多线图表 | `/network/:serverId` → 选择 24h → 图表显示多条彩色延迟线（每个目标一条） | — |
| N8 | 图表目标显隐 | 点击目标卡片上的眼睛图标 → 对应线条从图表中隐藏/显示 | — |
| N9 | 实时模式 | 详情页选择「实时」→ 等待 2 分钟 → 图表有新数据点持续追加 | — |
| N10 | 时间范围切换 | 依次点击 1h/6h/24h/7d/30d → 图表时间轴和数据正确切换 | — |
| N11 | Tooltip 展示 | 鼠标悬停图表 → Tooltip 显示时间戳 + 各目标延迟值 | — |
| N12 | 丢包显示 | 目标卡片显示丢包率百分比 → 100% 丢包时延迟显示 "N/A"，图表线条中断 | — |
| N13 | 异常摘要表 | 图表下方异常摘要表 → 显示高延迟/高丢包/不可达的异常记录（如存在） | — |
| N14 | 异常横幅 | 总览页 → 如有异常 VPS → 页面顶部显示黄色告警横幅 | — |
| N15 | CSV 导出 | 详情页 → 点击导出 CSV → 下载文件包含时间/目标/延迟/丢包数据 | — |
| N16 | 预设目标不可删 | 设置页 → 预设目标无删除按钮 → API 直接 DELETE 返回 403 | — |
| N17 | 能力控制 | 服务器禁用 CAP_PING_ICMP → Agent 停止该目标的 ICMP 探测 → 重新启用 → 恢复探测 | — |
| N18 | 告警规则类型 | `/settings/alerts` → 新建规则 → 类型下拉包含「Network Latency」和「Network Packet Loss」 | — |
| N19 | 底部统计栏 | 详情页底部显示：综合平均延迟 \| 可用性百分比 \| 目标数 n/n | — |
| N20 | 服务器信息栏 | 详情页显示 VPS 基本信息：IPv4、IPv6、地区、OS | — |
| N21 | 最大目标数限制 | 为某 VPS 配置超过 20 个目标 → API 返回错误 | — |
| N22 | i18n 切换 | 网络质量相关页面 → 中英文切换 → 标题/标签/按钮/Tooltip 全部正确翻译 | — |

### 验证清单 — Docker 容器监控

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| D1 | 能力开关（启用） | Server Detail → 启用 Docker Management capability → Docker 按钮出现 | ✅ |
| D2 | 能力开关（禁用） | 关闭 CAP_DOCKER → Docker 按钮消失 → API 返回 403 | ✅ |
| D3 | Docker 不可用 | Agent 无 Docker 环境 → Docker 页面显示 "Docker is not available" 占位 | ✅ |
| D4 | Docker 可用无容器 | Agent 有 Docker 但无容器 → 显示概览卡片 + "No containers found" | ✅ |
| D5 | 概览卡片 | 显示 5 张卡片：Running / Stopped / Total CPU / Total Memory / Docker Version | ✅ |
| D6 | 容器列表渲染 | 表格显示容器 Name / Image / Status / CPU% / Memory / Network I/O | ✅ |
| D7 | 容器搜索 | 输入容器名或镜像名 → 表格过滤匹配项 | ✅ |
| D8 | 容器过滤 | 点击 Running / Stopped / All 按钮 → 切换过滤状态 | ✅ |
| D9 | 容器详情弹窗 | 点击容器行 → 弹出 Dialog 显示元信息 + Stats + Logs | ✅ |
| D10 | 容器 Stats | 详情弹窗中 4 张迷你卡片：CPU / Memory（含进度条） / Net I/O / Block I/O | ✅ |
| D11 | 容器日志流 | 详情弹窗中日志区域自动连接 → 显示实时日志流 | ✅ |
| D12 | 日志 Follow | 开启 Follow → 新日志自动滚动到底部 → 关闭 Follow → 停止滚动 | ✅ |
| D13 | 日志 stderr 颜色 | stderr 日志行显示红色文本 | ✅ |
| D14 | 日志清除 | 点击 Clear → 日志区域清空 | ✅ |
| D15 | 日志连接状态 | 连接时绿色圆点 + "Connected" → 断开时灰色 + "Disconnected" | ✅ |
| D16 | 实时数据更新 | WS 推送 docker_update → 容器列表和 Stats 实时刷新 | ✅ |
| D17 | 事件时间线 | Docker 事件（start/stop/die 等）按时间倒序显示 → 相对时间戳 | ✅ |
| D18 | 事件 Badge | 事件类型 Badge：container/image/network/volume 各有不同样式 | ✅ |
| D19 | 网络列表弹窗 | 点击 Networks 按钮 → Dialog 显示网络 Name / Driver / Scope / 容器数 | ✅ |
| D20 | 卷列表弹窗 | 点击 Volumes 按钮 → Dialog 显示卷 Name / Driver / Mountpoint / 创建时间 | ✅ |
| D21 | 订阅/退订 | 进入 Docker 页 → WS 发送 docker_subscribe → 离开页面 → 发送 docker_unsubscribe | ✅ |
| D22 | docker_availability_changed | Agent Docker daemon 停止 → 页面切换为不可用占位 → daemon 恢复 → 页面自动恢复 | — |
| D23 | i18n 中文 | 切换中文 → 服务器详情页 Docker 按钮显示 "Docker"，能力名显示 "Docker 管理" | ✅ |
| D24 | i18n 英文 | 切换英文 → Docker 按钮显示 "Docker"，能力名显示 "Docker Management" | ✅ |

### 验证清单 — shadcn Chart 图表重构

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| C1 | MetricsChart 渲染 | `/servers/:id` → 实时模式 → CPU/Memory/Disk/Network/Load 图表正常渲染 | ✅ |
| C2 | MetricsChart Tooltip 格式化 | hover CPU 图表 → 显示 `xx.x%`；hover Network In → 显示 `xx.x MB/s` 格式 | ✅ |
| C3 | MetricsChart 历史切换 | 点击 1h/6h/24h → 图表切换为历史数据，X 轴时间格式正确 | — |
| C4 | LatencyChart 多线渲染 | `/network/:serverId` → 多个目标 → 图表显示多条彩色延迟线 | — |
| C5 | LatencyChart 目标隐藏/显示 | 点击 TargetCard 切换可见性 → 图表中对应线条正确隐藏/显示 | — |
| C6 | LatencyChart Tooltip 格式化 | hover 图表 → 显示 `xx.x ms` + 目标名称（非 target_id） | — |
| C7 | LatencyChart 颜色一致性 | TargetCard 颜色圆点与图表线条颜色一一对应 | — |
| C8 | TrafficCard 展开态 | 服务器详情页 → 点击展开 → 日 BarChart + 小时 LineChart 正常渲染 | ✅ |
| C9 | TrafficCard BarChart Legend | 日 BarChart 底部显示 ↓ In / ↑ Out 图例 | — |
| C10 | TrafficCard Tooltip 格式化 | hover 柱状图 → 显示格式化后的字节数（如 `12.5 MB`） | — |
| C11 | TrafficCard 颜色修复 | BarChart 柱状和 LineChart 线条颜色正确显示（修复前 hsl(oklch) 无效） | — |
| C12 | PingResultsChart 渲染 | `/settings/ping-tasks` → 展开任务 → 延迟图表正常渲染 | — |
| C13 | PingResultsChart 断线 | Ping 失败时间点显示为断线而非连续 | — |
| C14 | PingResultsChart Tooltip | hover 图表 → 显示 `xx.xms` + 完整日期时间 | — |
| C15 | 浅色主题 | 所有图表在浅色模式下颜色、背景、文字对比度正常 | ✅ |
| C16 | 深色主题 | 所有图表在深色模式下颜色、背景、文字对比度正常 | ✅ |
| C17 | recharts outline | 图表元素无多余 outline/focus ring（CSS hack 已删除） | — |
| C18 | Tooltip 多系列指示器 | LatencyChart/TrafficCard tooltip 行显示颜色指示器 + 系列标签（非纯文本） | — |

### 验证清单 — 文件管理

#### 基础功能

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| F1 | 能力开关（启用） | Server Detail → 启用 File Manager capability → Files 按钮出现 | — |
| F2 | 能力开关（禁用） | 关闭 CAP_FILE → Files 按钮消失 → 直接调 API 返回 403 | — |
| F3 | 文件浏览 | 点击 Files → 显示 root_paths 下文件列表（名称/大小/权限/修改时间） | — |
| F4 | 目录导航（进入） | 点击文件夹 → 进入子目录 → 面包屑导航更新 | — |
| F5 | 目录导航（返回） | 点击面包屑段落 → 跳转到对应层级 → 点击 `..` → 返回上级 | — |
| F6 | 空目录显示 | 进入空目录 → 显示 "Empty directory" 占位文字 | — |
| F7 | 目录排序 | 文件列表中目录排在前面，同类按名称字母排序 | — |

#### 文件查看 & 编辑

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| F8 | 文本文件查看 | 点击 .yaml/.json/.sh 文件 → Monaco Editor 显示内容 + 语法高亮正确 | — |
| F9 | 语法高亮映射 | 分别打开 .yaml/.json/.ts/.py/.rs/.toml/.sh/.go → 语言标识正确 | — |
| F10 | 文件编辑保存（Ctrl+S） | 编辑文本 → Ctrl+S → toast 提示 "File saved" → 重新打开验证 | — |
| F11 | 文件编辑保存（按钮） | 编辑文本 → 点击 Save 按钮 → 保存成功 | — |
| F12 | 保存冲突检测 | 编辑文件 → 另一端修改同文件 → 保存 → 弹出 "File modified externally, overwrite?" | — |
| F13 | 大文件不预览 | 点击 >384KB 的文件 → 显示文件信息 + Download 按钮（不加载编辑器） | — |
| F14 | Monaco 主题同步 | 切换深色/浅色主题 → Monaco Editor 主题跟随变化 | — |

#### 文件操作

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| F15 | 新建目录 | 点击 New Folder → 输入名称 → 创建 → 列表显示新目录 | — |
| F16 | 文件删除 | 右键 → Delete → 确认对话框 → 确认 → 文件从列表消失 | — |
| F17 | 目录删除（递归） | 右键非空目录 → Delete → 确认 → 目录及其内容全部删除 | — |
| F18 | 文件重命名 | 右键 → Rename → 输入新名称 → 文件名更新 | — |
| F19 | 复制路径 | 右键 → Copy Path → 剪贴板包含完整路径 | — |

#### 上传 & 下载

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| F20 | 文件上传（选择） | 点击 Upload → 选择文件 → 上传成功 → 列表显示新文件 | — |
| F21 | 文件上传（拖拽） | 拖拽文件到上传区域 → 上传成功 | — |
| F22 | 文件下载 | 右键 → Download → 浏览器开始下载 → transfer bar 显示进度 | — |
| F23 | 大文件传输进度 | 上传/下载 >10MB 文件 → transfer bar 显示进度百分比和字节数 | — |
| F24 | 取消传输 | 传输进行中 → 点击取消按钮 → 传输停止 | — |
| F25 | 传输列表 | GET /api/files/transfers → 返回当前所有传输状态 | — |

#### 安全 & 权限

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| F26 | 路径沙箱 | Agent 配置 root_paths=["/home"] → 尝试访问 /etc → 被拒绝 "path outside allowed roots" | — |
| F27 | 路径穿越防护 | 尝试访问 `/home/../../etc/passwd` → canonicalize 后被拒绝 | — |
| F28 | deny_patterns 拦截 | 尝试读取 .env / *.key / *.pem / id_rsa / shadow → Agent 拒绝 "file type blocked" | — |
| F29 | 空 root_paths 全拒绝 | Agent 配置 root_paths=[] → 所有文件操作被拒绝 | — |
| F30 | Member 用户只读 | member 角色 → 可浏览/读取 → 写入/删除/创建返回 403 | — |
| F31 | 审计日志记录 | 执行 write/delete/upload/download → 审计日志出现对应 file_* 记录 | — |
| F32 | 离线服务器 | Agent 离线 → 文件操作返回 404 "Server offline" | — |
| F33 | 并发传输限制 | 同时发起 4 个下载 → 第 4 个返回 429 "Too many concurrent transfers" | — |

#### i18n & UI

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| F34 | 中文模式 | 切换中文 → 文件管理页面标题/按钮/对话框/toast 全部显示中文 | — |
| F35 | 英文模式 | 切换英文 → 所有 UI 元素显示英文 | — |
| F36 | 传输状态标签 | 传输状态 pending/in_progress/ready/failed 显示正确的本地化文本 | — |

### 验证清单 — 月度流量统计

#### 单元测试覆盖（自动化）

以下单元测试位于 `crates/server/src/service/traffic.rs`，通过 `cargo test -p serverbee-server service::traffic` 运行：

| 测试组 | 测试名 | 验证内容 |
|--------|--------|----------|
| **增量计算** | `test_compute_delta_normal` | 正常增量：curr - prev |
| | `test_compute_delta_both_restart` | 双方向同时重启（curr < prev）→ 使用 curr 原始值 |
| | `test_compute_delta_single_direction_restart_in` | 仅入站重启，出站正常 |
| | `test_compute_delta_single_direction_restart_out` | 仅出站重启，入站正常 |
| | `test_compute_delta_zero` | 无变化 → delta 为 0 |
| **计费周期** | `test_cycle_range_natural_month` | 自然月（anchor=1）→ 3/1~3/31 |
| | `test_cycle_range_billing_day_15` | 自定义起始日 15 → 3/15~4/14 |
| | `test_cycle_range_billing_day_before_anchor` | 当前日 < anchor → 回退到上一周期 |
| | `test_cycle_range_quarterly` | 季付周期 → 4/1~6/30 |
| | `test_cycle_range_yearly` | 年付周期 → 1/1~12/31 |
| | `test_cycle_range_unknown_falls_back_to_monthly` | 未知周期类型 → 回退到月付 |
| **预测算法** | `test_prediction_normal` | 正常预测：7 天数据 → 预计超限 |
| | `test_prediction_too_early` | 不足 3 天 → 返回 None |
| | `test_prediction_no_limit` | 无流量限额 → 返回 None |
| **DB 操作** | `test_upsert_traffic_hourly_accumulates` | 同一小时两次 upsert → bytes 累加 |
| | `test_load_transfer_cache_from_traffic_state` | traffic_state 表 → HashMap 缓存加载 |
| | `test_aggregate_daily_timezone_bucketing` | Asia/Shanghai 时区 → UTC 小时正确聚合到本地日期 |

#### 集成测试覆盖（自动化）

以下集成测试位于 `crates/server/tests/integration.rs`，启动真实 Server + SQLite 临时数据库：

| 测试名 | 流程 |
|--------|------|
| `test_oneshot_task_backward_compat` | migration 新增 NOT NULL 列（带默认值）后 → 一次性任务仍可正常创建 |
| `test_traffic_api_returns_data` | 注册 Agent → `GET /api/servers/{id}/traffic` → 验证响应包含 cycle_start/cycle_end/bytes_*/daily/hourly |
| `test_server_billing_start_day` | 更新 billing_start_day=15 → GET server 验证持久化 → 流量 API 反映 traffic_limit |

#### E2E 手动验证

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| T1 | 流量数据写入 | Agent 连接并上报 → 等待 60s → `traffic_hourly` 表有记录 | — |
| T2 | 增量正确性 | Agent 上报 net_in_transfer=1000 → 60s 后上报 1500 → hourly delta=500 | — |
| T3 | Agent 重启恢复 | 停止 Agent → 重启 → 上报的 cumulative 从 0 开始 → delta 使用原始值（不产生负数） | — |
| T4 | 日聚合 | 等待 aggregator 运行（每小时）→ `traffic_daily` 表有记录 → bytes 与 hourly SUM 一致 | — |
| T5 | 时区聚合 | 配置 `SERVERBEE_SCHEDULER__TIMEZONE=Asia/Shanghai` → UTC 20:00 的 hourly 记录归入次日（本地 04:00） | — |
| T6 | 流量 API 响应 | `GET /api/servers/{id}/traffic` → 返回 cycle_start/end、bytes_in/out/total、daily[]、hourly[] | — |
| T7 | BillingInfoBar 进度条 | 设置 traffic_limit → 服务器详情页显示进度条（used/limit + 百分比） | — |
| T8 | 进度条颜色阈值 | 0-70% 绿色 → 70-90% 黄色 → 90%+ 红色 | — |
| T9 | 预测标记 | 已过 3 天 + 设有 traffic_limit → 进度条显示虚线预测标记 | — |
| T10 | 超限警告 | 预测超限 → 进度条下方显示红色警告文字 | — |
| T11 | TrafficCard 折叠展开 | 点击流量统计卡片 → 展开显示日柱状图 + 小时折线图 → 再次点击折叠 | — |
| T12 | 日柱状图 | 展开 TrafficCard → 柱状图显示每日 in/out 堆叠 → hover 显示 tooltip | — |
| T13 | 小时折线图 | 展开 TrafficCard → 折线图显示今日小时 in/out → X 轴 HH:00 格式 | — |
| T14 | 无流量时隐藏 | 新注册服务器（无流量数据）→ TrafficCard 不显示 | — |
| T15 | 编辑 billing_start_day | 编辑对话框 → 设置计费起始日为 15 → 保存 → 流量 API 周期变为 15~14 | — |
| T16 | billing_start_day 范围 | 输入 0 或 29 → 保存失败（trigger 拦截） | — |
| T17 | 自然月回退 | 清空 billing_start_day → 保存 → 周期回到 1~月末 | — |
| T18 | 流量限额类型 | 设置 traffic_limit_type=up → 进度条仅显示上传用量 | — |
| T19 | 数据保留清理 | 配置 `traffic_hourly_days=1` → 等待 cleanup 运行 → 超过 1 天的 hourly 记录被删除 | — |
| T20 | 告警集成 | 创建 transfer_all_cycle 告警规则 → cycle_interval=billing → 流量超限时触发告警 | — |
| T21 | i18n 中文 | 切换中文 → "流量统计"、"每日流量"、"今日小时流量"、"预计将超出限额" 正确显示 | — |
| T22 | i18n 英文 | 切换英文 → "Traffic Statistics"、"Daily Traffic"、"Today's Hourly Traffic" 正确显示 | — |

### 验证清单 — 定时任务

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| S1 | Tab 切换 | `/settings/tasks` → 点击 Scheduled tab → 显示定时任务列表（初始为空） | — |
| S2 | 创建定时任务 | 点击 Create → 填写名称/cron/命令/服务器/超时 → 创建 → 列表出现新任务 | — |
| S3 | Cron 验证 | 输入无效 cron → 显示错误提示 → 输入有效 cron → 错误消失 | — |
| S4 | 编辑任务 | 点击 Edit → 修改名称和 cron → 保存 → 列表更新 | — |
| S5 | 暂停/恢复 | 点击 Pause → 显示 "Paused" 标签 → 点击 Resume → 标签消失 | — |
| S6 | 手动执行 | 点击 Run Now → toast "Task triggered" → 展开查看执行结果 | — |
| S7 | 重复执行 409 | 手动执行 → 立即再次点击 Run Now → toast "Task is currently running" | — |
| S8 | 执行历史 | 点击任务行展开 → 显示按 run_id 分组的执行记录 + 每服务器结果 | — |
| S9 | 删除任务 | 点击 Delete → 确认弹窗 → 确认 → 任务从列表消失 | — |
| S10 | Cron 自动触发 | 创建每分钟任务 (0 * * * * *) → 等待 60s+ → 自动执行 → 历史中出现记录 | — |
| S11 | 重试机制 | 创建任务 retry_count=2 → 目标服务器离线 → 3 次尝试记录 (exit_code=-3) | — |
| S12 | CAP_EXEC 检查 | 目标服务器禁用 CAP_EXEC → 执行 → 结果 exit_code=-2 "Capability denied" | — |
| S13 | next_run_at 更新 | 任务执行后 → 列表 Next 列更新为下次执行时间 | — |
| S14 | i18n 中文 | 切换中文 → Tab 显示 "即时命令"/"定时任务"，按钮/标签显示中文 | — |
| S15 | i18n 英文 | 切换英文 → Tab 显示 "One-shot"/"Scheduled"，所有 UI 英文 | — |

### 验证清单 — 服务监控 (Service Monitor)

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| SM1 | 创建 SSL 监控 | `/settings/service-monitors` → Add → 类型 SSL → 输入域名 → 创建 → 列表出现 | — |
| SM2 | 创建 TCP 监控 | 类型 TCP → 输入 host:port → 创建 → 状态显示 OK/FAIL | — |
| SM3 | 创建 HTTP Keyword 监控 | 类型 HTTP → 输入 URL + 关键词 → 创建 → 检测响应体含关键词 | — |
| SM4 | 创建 DNS 监控 | 类型 DNS → 输入域名 + 期望 IP → 创建 → 解析结果与预期匹配则 OK | — |
| SM5 | 创建 WHOIS 监控 | 类型 WHOIS → 输入域名 → 创建 → 显示到期剩余天数 | — |
| SM6 | 手动触发检测 | 点击 Check Now → toast 提示 → 状态和最后检测时间更新 | — |
| SM7 | 编辑监控 | 点击 Edit → 修改名称/阈值 → 保存 → 列表更新 | — |
| SM8 | 删除监控 | 点击 Delete → 确认 → 从列表消失 → 历史记录级联删除 | — |
| SM9 | 状态徽章 | OK = 绿色，FAIL = 红色，PENDING = 灰色 | — |
| SM10 | 详情页 — 状态图表 | 点击监控 → 详情页 → 显示历史状态时间线图表 | — |
| SM11 | 详情页 — 记录表格 | 详情页显示历史检测记录（时间/状态/延迟/消息）+ 时间范围过滤 | — |
| SM12 | 自动调度 | 创建监控 → 等待 interval 秒 → 自动执行 → 历史记录出现新条目 | — |
| SM13 | SSL 到期警告 | SSL 证书剩余天数 < threshold → 状态变为 FAIL | — |
| SM14 | WHOIS 到期警告 | 域名到期剩余天数 < threshold → 状态变为 FAIL | — |
| SM15 | 记录清理 | 配置 service_monitor_record_days=1 → cleanup 运行后旧记录删除 | — |
| SM16 | i18n 中文 | 切换中文 → 页面标题/按钮/状态标签全部显示中文 | — |
| SM17 | i18n 英文 | 切换英文 → 所有 UI 元素显示英文 | — |

### 验证清单 — 告警 & 通知全链路

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| A1 | 通知渠道创建 | 创建 Webhook + Telegram → 列表显示 2 个渠道 | ✅ |
| A2 | 通知组创建 | 创建 "E2E Test Group" 关联 2 个渠道 → 列表显示 "2 channel(s)" | ✅ |
| A3 | 测试通知发送 | 点击测试按钮 → Webhook (webhook.site) + Telegram 均收到消息 | ✅ |
| A4 | 阈值告警触发 | 创建 cpu ≥ 1% 规则 → 60s 后触发 → Webhook + Telegram 收到告警通知 | ✅ |
| A5 | 告警状态展示 | 点击 States → 显示 "New Server" 🔴 Triggered (2x) + 时间戳 | ✅ |
| A6 | 告警条件格式 | 规则摘要正确显示 `cpu ≥ 1 | always` 和 `offline 30s | once` | ✅ |
| A7 | 离线告警触发 | 创建 offline 30s 规则 → 停 Agent → 等待触发 | ⚠️ 未触发（时序窗口问题，非代码 bug） |
| A8 | Swagger UI | 访问 `/swagger-ui/` → 显示 ServerBee API 0.1.0 OAS 3.1 | ✅ |
| A9 | Ping 任务创建 | 创建 HTTP ping → 列表显示 "Ping Google" | ✅ |
| A10 | Ping 结果收集 | 等待 25s → 7 条记录，全部成功，延迟 387-402ms | ✅ |
| A11 | Capabilities API 修复 | `update_server`/`batch_capabilities` Extension bug 已修复 → API 正常返回 | ✅ |
| A12 | 终端页面加载 | 启用 CAP_TERMINAL → 打开 `/terminal/:id` → xterm.js 容器渲染正常 | ✅ |
| A13 | 终端 WS 连接 | WebSocket 连接状态显示 "closed" — Agent 需要重连以获取 CapabilitiesSync | ⚠️ 需 Agent 重连 |

### E2E 测试中发现并修复的 Bug

| Bug | 描述 | 修复 |
|-----|------|------|
| 登录错误消息 | 显示原始 JSON `{"error":{"code":"UNAUTHORIZED",...}}` | 解析 JSON 提取 `error.message` 字段 (`69af3e7`) |
| 通知表单明文密码 | password/bot_token/device_key 使用 `type="text"` | 改为 `type="password"` 掩码 (`82dcf15`) |
| 告警表单缺失字段 | 仅 12 种规则类型 + 仅 `max` 字段 | 扩展到 19 种 + 条件 min/duration/cycle 字段 |
| 告警状态无 UI | 后端有 alert_state 但前端无展示 | 新增 API + 可展开 per-server 状态面板 (`a8defea`) |
| Capabilities API 500 | `update_server`/`batch_capabilities` 使用 `Extension<(String,String,String)>` 无人注入 | 改为 `Extension<CurrentUser>` + `HeaderMap` |

## 测试文件位置

```
crates/common/src/constants.rs          # 能力常量测试（含 CAP_DOCKER）
crates/common/src/protocol.rs           # 协议序列化测试（含 Docker 消息变体）
crates/common/src/docker_types.rs      # Docker 数据结构序列化测试
crates/common/src/types.rs             # SystemInfo features 字段测试
crates/server/src/service/alert.rs      # 告警服务测试
crates/server/src/service/auth.rs       # 认证服务测试 (含 DB 集成)
crates/server/src/service/notification.rs # 通知服务测试
crates/server/src/service/record.rs     # 记录服务测试
crates/server/src/service/agent_manager.rs # AgentManager 单元测试 (含 per-entry TTL)
crates/server/src/service/task_scheduler.rs # TaskScheduler 单元测试 (3 tests)
crates/server/src/task/task_scheduler.rs   # 定时任务执行流程测试 (2 tests)
crates/server/src/service/docker_viewer.rs # DockerViewerTracker 单元测试
crates/server/src/service/server.rs     # 服务器 CRUD 测试
crates/server/src/service/user.rs       # 用户服务测试
crates/server/src/service/ping.rs       # Ping 服务测试
crates/server/src/middleware/auth.rs    # 中间件 Cookie/Key 提取测试
crates/server/src/test_utils.rs         # 测试辅助 (setup_test_db)
crates/server/tests/integration.rs      # 集成测试 (26 tests)
crates/server/tests/docker_integration.rs # Docker 集成测试 (4 tests)
crates/agent/src/collector/tests.rs     # Agent 采集器测试
crates/agent/src/pinger.rs              # Agent Pinger 测试
crates/agent/src/probe_utils.rs         # 批量探测解析测试
crates/agent/src/network_prober.rs      # 网络探测模块测试
crates/server/src/presets/mod.rs            # 预设目标加载测试
crates/server/src/service/network_probe.rs # 网络探测服务单元测试
crates/server/src/service/file_transfer.rs # 文件传输管理器测试 (9 tests)
crates/server/src/service/traffic.rs       # 流量统计服务测试 (17 tests)
crates/server/src/config.rs                # 配置测试 (1 test)
crates/agent/src/file_manager.rs           # Agent 文件管理器测试 (24 tests)
crates/server/src/service/service_monitor.rs # 服务监控 CRUD + 记录管理测试 (15 tests)
crates/server/src/service/checker/dns.rs   # DNS checker 单元测试 (3 tests)
crates/server/src/service/checker/http_keyword.rs # HTTP Keyword checker 单元测试 (3 tests)
crates/server/src/service/checker/ssl.rs   # SSL checker 单元测试 (3 tests)
crates/server/src/service/checker/tcp.rs   # TCP checker 单元测试 (3 tests)
crates/server/src/service/checker/whois.rs # WHOIS checker 单元测试 (8 tests)
apps/web/src/hooks/use-terminal-ws.test.ts # Terminal WS hook 测试
apps/web/src/lib/capabilities.test.ts   # 能力位测试
apps/web/src/lib/api-client.test.ts     # API Client 测试
apps/web/src/lib/utils.test.ts          # 工具函数测试
apps/web/src/lib/ws-client.test.ts      # WebSocket Client 测试
apps/web/src/hooks/use-auth.test.tsx    # Auth hook 测试
apps/web/src/hooks/use-api.test.tsx     # API hook 测试
apps/web/src/hooks/use-realtime-metrics.test.tsx # 实时指标 hook 测试（纯函数 + renderHook 集成）
apps/web/src/hooks/use-servers-ws.test.ts # WS 数据合并测试
apps/web/src/lib/file-utils.test.ts     # 文件工具函数测试
apps/web/src/hooks/use-traffic.test.tsx # 流量 hook 测试 (3 tests)
apps/web/vitest.config.ts               # Vitest 配置
.github/workflows/ci.yml               # CI 流水线
```
