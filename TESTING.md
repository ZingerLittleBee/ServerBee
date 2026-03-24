# ServerBee 测试指南

## 快速命令

```bash
# 全量测试
cargo test --workspace && bun run test

# Rust 测试
cargo test --workspace

# 前端测试
bun run test

# 代码质量
cargo clippy --workspace -- -D warnings
bun x ultracite check
bun run typecheck
```

## Rust 测试

### 按 crate 运行

```bash
cargo test -p serverbee-common          # 协议 + 能力常量 + Docker 类型 + Traceroute (43 tests)
cargo test -p serverbee-server          # 服务端单元 + 集成 + dashboard + uptime + geoip (227 unit + 39 integration + 4 docker = 270 tests)
cargo test -p serverbee-agent           # Agent 采集器 + Pinger + NetworkProber + FileManager + Traceroute (56 tests)
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
| `common/protocol.rs` | 24 | 消息序列化/反序列化（NetworkProbe + 文件管理 + Docker + IpChanged/ServerIpChanged + Report.disk_io 覆盖） |
| `common/docker_types.rs` | 3 | Docker 容器/动作/日志条目序列化/反序列化 |
| `common/types.rs` | 4 | SystemInfo features 字段默认值和序列化、NetworkInterface 序列化、SystemReport.disk_io 向后兼容 |
| `server/service/geoip.rs` | 4 | 加载不存在路径返回 None、无效 MMDB 数据返回 Err、IPv4 私有地址判定、IPv6 私有地址判定 |
| `server/service/alert.rs` | 22 | 阈值判定、指标提取、采样窗口、事件驱动规则类型、服务器覆盖判定、AlertStateManager 构造、list_events 聚合与分页 |
| `server/service/auth.rs` | 19 | 密码哈希、session、API key、TOTP、登录、改密 |
| `server/service/notification.rs` | 16 | 模板变量替换、渠道配置解析 |
| `server/service/record.rs` | 8 | 历史查询、聚合、清理策略、保存上报、过期清理、disk_io_json 持久化与小时聚合 |
| `server/service/agent_manager.rs` | 13 | 连接管理、广播、缓存、终端会话、离线检测、请求-响应中继、per-entry TTL |
| `server/service/docker_viewer.rs` | 5 | 首位/末位观察者检测、has_viewers、批量连接移除、批量服务器移除 |
| `server/service/server.rs` | 5 | 服务器 CRUD、批量删除 |
| `server/service/user.rs` | 4 | 用户 CRUD、级联删除、最后 admin 保护 |
| `server/service/ping.rs` | 3 | Ping 任务 CRUD |
| `server/middleware/auth.rs` | 6 | Cookie/API Key 提取 |
| `agent/collector/` | 9 | 系统信息、指标范围、使用量约束、磁盘 I/O 基线语义、设备过滤、速率计算排序、mount-path key 速率验证 |
| `agent/pinger.rs` | 2 | TCP 探测（开放/关闭端口） |
| `agent/config.rs` | 1 | IpChangeConfig 默认值 |
| `server/service/audit.rs` | 3 | 审计日志记录、列表、排序 |
| `server/service/config.rs` | 5 | KV 存取、upsert、类型化读写 |
| `server/presets/mod.rs` | 8 | 预设目标加载、ID 唯一性、查找、分组元数据、探测类型校验 |
| `server/service/network_probe.rs` | 13 | 网络探测目标 CRUD、预设目标保护、default_target_ids 校验、server targets 分配 |
| `agent/probe_utils.rs` | 2 | 批量探测结果解析、地址解析 |
| `agent/network_prober.rs` | 2 | 网络探测任务调度、结果上报 |
| `server/service/file_transfer.rs` | 9 | 传输创建/获取、并发限制、过期清理、状态转换、进度更新、临时文件清理 |
| `agent/file_manager.rs` | 24 | 路径校验、目录列表、文件读写、删除/创建目录/重命名、上传/下载 |
| `server/service/traffic.rs` | 21 | 增量计算、计费周期、预测算法、DB 操作、overview/history、序列化 |
| `server/service/task_scheduler.rs` | 3 | TaskScheduler 创建、重叠检测、取消活跃运行 |
| `server/task/task_scheduler.rs` | 2 | correlation_id 格式、唯一性 |
| `server/config.rs` | 1 | 时区解析 |
| `server/service/service_monitor.rs` | 15 | CRUD、记录管理、check_state 更新、清理策略、级联删除、JSON server_ids |
| `server/service/checker/dns.rs` | 3 | 自定义 nameserver、无效 nameserver、系统默认 resolver |
| `server/service/checker/http_keyword.rs` | 3 | 空 headers、无效 header 值、带自定义 headers |
| `server/service/checker/ssl.rs` | 3 | 默认端口解析、显式端口解析、config 覆盖端口 |
| `server/service/checker/tcp.rs` | 3 | 连接拒绝、连接成功、默认超时值 |
| `server/service/checker/whois.rs` | 8 | 多种日期格式解析、paid-till、ISO、注册商解析、长文本截断 |
| `server/service/dashboard.rs` | 12 | 仪表盘 CRUD、默认仪表盘、widget diff、排序、删除保护、set_default |
| `server/service/uptime.rs` | 5 | get_daily_filled 精确天数、日期边界验证、部分数据补零、单日/90天查询 |

### 集成测试覆盖

| 测试 | 流程 |
|------|------|
| `test_agent_register_connect_report` | Agent 注册 → WS 连接 → SystemInfo → 指标上报 |
| `test_server_records_api_returns_disk_io_json` | 注册 Agent → 保存带 disk_io 的记录 → GET records 返回 disk_io_json |
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
| `test_preset_target_source_field` | 验证预设 source/source_name 字段、自建目标 source 为 null |
| `test_preset_target_cannot_be_updated` | PUT 预设目标 → 返回 403 |
| `test_file_list_server_offline` | 启用 CAP_FILE → POST /files/list → 离线返回 404 |
| `test_file_capability_enforcement` | CAP_DEFAULT → POST /files/list → 403 |
| `test_file_transfers_endpoint` | GET /files/transfers → 空列表 → DELETE 不存在 → 404 |
| `test_file_write_requires_admin` | member POST /files/write → 403 |
| `test_file_delete_requires_admin` | member POST /files/delete → 403 |
| `test_file_mkdir_requires_admin` | member POST /files/mkdir → 403 |
| `test_oneshot_task_backward_compat` | 新 migration 后一次性任务仍可正常创建 |
| `test_service_monitor_crud_and_check` | 创建 TCP 监控 → 列表 → 触发检查 → 验证记录 → 删除 |
| `test_traffic_overview_api` | 空 overview → 注册 Agent + 配置 billing → overview 包含服务器 → daily API |
| `test_traffic_api_returns_data` | 注册 Agent → 查询流量 API → 验证响应结构 |
| `test_server_billing_start_day` | 更新 billing_start_day → 验证持久化和流量 API |
| `test_dashboard_crud_cycle` | 创建仪表盘 → 列表 → 更新 (widget diff) → 删除 |
| `test_dashboard_default_auto_creates` | GET default → 自动创建 → 幂等 |
| `test_dashboard_rbac_member_cannot_write` | member POST/PUT/DELETE → 403 |
| `test_alert_events_endpoint` | 创建告警规则 → GET /alert-events → 验证响应 |
| `test_uptime_daily_requires_auth` | 无认证访问 uptime-daily → 401 |
| `test_uptime_daily_server_not_found` | 认证后访问不存在 server → 404 |
| `test_uptime_daily_returns_data` | 注册 Agent → days=0→400、days=366→400、默认→200 (90 条) |
| `test_geoip_status_endpoint` | GET /api/geoip/status → 200、installed=false |
| `test_geoip_status_accessible_by_member` | member 访问 geoip status → 200 |
| `test_geoip_download_requires_admin` | member POST /api/geoip/download → 403 |

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
| `use-realtime-metrics.test.tsx` | 13 | toRealtimeDataPoint 转换、hook 集成 |
| `use-servers-ws.test.ts` | 8 | 数据合并、静态字段保护、在线状态切换 |
| `use-terminal-ws.test.ts` | 20 | WS URL 构造、状态机、base64 编码、resize |
| `file-utils.test.ts` | 30 | 扩展名→语言映射、文本文件判定、图片文件判定 |
| `use-traffic.test.tsx` | 3 | 流量数据获取、查询 key 验证、空 serverId 禁用 |
| `traffic-card.test.tsx` | 1 | TrafficCard 渲染、tab 切换 |
| `disk-io.test.ts` | 3 | disk_io_json 解析、汇总序列、按磁盘补零序列 |
| `disk-io-chart.test.tsx` | 2 | DiskIoChart 视图切换、空数据返回 null |
| `dashboard-layout.test.ts` | 4 | widgetsToLayout/layoutToPatch/mergeLayoutPatch 转换 |
| `use-dashboard-editor.test.tsx` | 9 | 编辑草稿生命周期、layout patch 合并 |
| `use-dashboard.test.tsx` | 8 | Dashboard 查询/变更 hooks |
| `widget-renderer.test.tsx` | 14 | 13 种 widget 类型渲染 + 未知类型 fallback |
| `dashboard-grid.test.tsx` | 9 | 拖拽/缩放交互、移动端单列布局 |
| `dashboard-editor-view.test.tsx` | 7 | Save/Cancel 编排、widget 添加/编辑 |
| `routes/_authed/-index.test.tsx` | 1 | dashboard 切换时 activeDashboardId 稳定 |
| `widget-config-dialog.test.tsx` | 9 | 各 widget 类型配置对话框 |
| `markdown.test.ts` | 8 | Markdown→HTML + XSS 防护 |
| `capabilities-dialog.test.tsx` | 1 | 能力控制对话框 |
| `uptime-timeline.test.tsx` | 11 | 分段/颜色/阈值/图例/补零/聚合 |
| `geoip.test.tsx` | 7 | GeoIP 设置页状态渲染 |

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

## 启动本地环境

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

> **注意**：`SERVERBEE_SERVER_URL` 应设置为 HTTP 基础地址（如 `http://127.0.0.1:9527`），Agent 会自动拼接 `/api/agent/register` 和 `/api/agent/ws?token=` 路径。

## 浏览器自动化测试（agent-browser）

使用 [agent-browser](https://github.com/anthropics/agent-browser) CLI 进行 E2E 浏览器测试。

### 安装

```bash
npm i -g agent-browser
# 或
brew install agent-browser

agent-browser install  # 安装 Chrome（如果没有）
```

### 自动化测试流程

```bash
# 1. 构建前端 + 后端
cd apps/web && bun install && bun run build && cd ../..
cargo build --workspace

# 2. 启动 server
SERVERBEE_ADMIN__PASSWORD=admin123 SERVERBEE_AUTH__SECURE_COOKIE=false cargo run -p serverbee-server &
sleep 8

# 3. 登录
agent-browser open http://localhost:9527/login
agent-browser snapshot -i
agent-browser fill @e1 "admin" && agent-browser fill @e2 "admin123" && agent-browser click @e3
agent-browser wait --load networkidle

# 4. Service Monitor — 创建 TCP 监控
agent-browser open http://localhost:9527/settings/service-monitors
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser click @e20  # Add Monitor
agent-browser wait 1000 && agent-browser snapshot -i
agent-browser click @e2  # Type dropdown
agent-browser wait 500 && agent-browser snapshot -i
agent-browser click @e7  # TCP option
agent-browser wait 500 && agent-browser snapshot -i
agent-browser fill @e3 "127.0.0.1:9527"  # Target
agent-browser click @e7  # Create
agent-browser wait --load networkidle
agent-browser snapshot -i
agent-browser click @e24  # Trigger check
agent-browser wait 2000
agent-browser click @e22  # View details
agent-browser wait --load networkidle
agent-browser screenshot /tmp/sm-detail.png

# 5. Traffic — 查看流量总览
agent-browser open http://localhost:9527/traffic
agent-browser wait --load networkidle
agent-browser screenshot /tmp/traffic.png

# 6. IP Changed — 验证告警规则类型
agent-browser open http://localhost:9527/settings/alerts
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser click @e20  # Add
agent-browser wait 1000 && agent-browser snapshot -i
agent-browser click @e26  # Conditions dropdown
agent-browser wait 500 && agent-browser snapshot -i
# 验证："IP Changed" 选项存在于列表末尾

# 7. Uptime Timeline — 公共状态页时间线
agent-browser open http://localhost:9527/status
agent-browser wait --load networkidle
agent-browser screenshot /tmp/p18-status-simple.png
agent-browser open http://localhost:9527/settings/status-pages
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser screenshot /tmp/p18-status-admin.png
agent-browser open http://localhost:9527/servers
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser click @e1
agent-browser wait --load networkidle
agent-browser screenshot /tmp/p18-server-detail.png
agent-browser open http://localhost:9527
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser screenshot /tmp/p18-dashboard.png

# 8. 清理
agent-browser close
pkill -f "target/debug/serverbee-server"
```

### 最近一次自动化测试结果（2026-03-19）

| 测试项 | 结果 |
|--------|------|
| Service Monitor 列表页渲染 | ✅ |
| 创建 TCP 监控 | ✅ |
| 触发手动检查 | ✅ |
| 详情页渲染 | ✅ |
| Traffic 总览页渲染 | ✅ |
| 告警 IP Changed 规则类型 | ✅ |
| 侧边栏导航 | ✅ |

## E2E 手动验证清单（按功能页面）

详细测试用例已拆分到 `tests/` 目录，每个文件对应一个功能页面：

| 文件 | 功能 | 路由 |
|------|------|------|
| [tests/auth-users.md](tests/auth-users.md) | 认证、用户与安全 | `/login`, `/settings/users`, `/settings/api-keys` |
| [tests/dashboard.md](tests/dashboard.md) | 自定义仪表盘 | `/` |
| [tests/server-detail.md](tests/server-detail.md) | 服务器列表与详情 | `/servers`, `/servers/:id` |
| [tests/network-quality.md](tests/network-quality.md) | 网络质量监控 | `/network`, `/network/:id`, `/settings/network-probes` |
| [tests/docker.md](tests/docker.md) | Docker 容器监控 | `/servers/:id/docker` |
| [tests/disk-io.md](tests/disk-io.md) | 磁盘 I/O 监控 | `/servers/:id` (历史模式) |
| [tests/traffic.md](tests/traffic.md) | 月度流量统计 | `/traffic`, `/servers/:id` (Traffic tab) |
| [tests/file-manager.md](tests/file-manager.md) | 文件管理 | `/servers/:id` (Files) |
| [tests/service-monitor.md](tests/service-monitor.md) | 服务监控 | `/settings/service-monitors`, `/service-monitors/:id` |
| [tests/scheduled-tasks.md](tests/scheduled-tasks.md) | 定时任务 | `/settings/tasks` (Scheduled tab) |
| [tests/alerts-notifications.md](tests/alerts-notifications.md) | 告警 & 通知 + IP 变更 | `/settings/alerts`, `/settings/notifications` |
| [tests/uptime.md](tests/uptime.md) | Uptime 90 天时间线 | `/status/:slug`, `/servers/:id`, Dashboard widget |
| [tests/geoip.md](tests/geoip.md) | GeoIP 数据库管理 | `/settings/geoip` |
| [tests/status-page.md](tests/status-page.md) | 状态页增强 | `/status/:slug`, `/settings/status-pages` |
| [tests/appearance.md](tests/appearance.md) | 主题、品牌、响应式 | `/settings/appearance` |
| [tests/i18n.md](tests/i18n.md) | 国际化 | 全站 |
| [tests/performance.md](tests/performance.md) | 前端性能测试 | `/servers/:id` (realtime) |

### 页面渲染快速验证

| 功能 | 路由 | 状态 |
|------|------|------|
| 登录 | `/login` | ✅ |
| Dashboard | `/` | — |
| Servers 列表 | `/servers` | ✅ |
| 服务器详情 | `/servers/:id` | — |
| 网络质量总览 | `/network` | — |
| 网络质量详情 | `/network/:id` | — |
| Docker 监控 | `/servers/:id/docker` | — |
| 流量总览 | `/traffic` | ✅ |
| 服务监控 | `/settings/service-monitors` | ✅ |
| 用户管理 | `/settings/users` | ✅ |
| 通知 | `/settings/notifications` | ✅ |
| 告警 | `/settings/alerts` | ✅ |
| API Keys | `/settings/api-keys` | ✅ |
| Security | `/settings/security` | ✅ |
| 审计日志 | `/settings/audit-logs` | ✅ |
| 远程命令 | `/settings/tasks` | ✅ |
| 公共状态页 | `/status` | ✅ |
| Swagger UI | `/swagger-ui/` | — |
| 终端 | `/terminal/:id` | — |

## 测试文件位置

```
crates/common/src/constants.rs          # 能力常量测试（含 CAP_DOCKER）
crates/common/src/protocol.rs           # 协议序列化测试
crates/common/src/docker_types.rs      # Docker 数据结构序列化测试
crates/common/src/types.rs             # SystemInfo + SystemReport 兼容测试
crates/server/src/service/alert.rs      # 告警服务测试
crates/server/src/service/auth.rs       # 认证服务测试
crates/server/src/service/notification.rs # 通知服务测试
crates/server/src/service/record.rs     # 记录服务测试
crates/server/src/service/agent_manager.rs # AgentManager 单元测试
crates/server/src/service/task_scheduler.rs # TaskScheduler 单元测试
crates/server/src/task/task_scheduler.rs   # 定时任务执行流程测试
crates/server/src/service/docker_viewer.rs # DockerViewerTracker 单元测试
crates/server/src/service/server.rs     # 服务器 CRUD 测试
crates/server/src/service/user.rs       # 用户服务测试
crates/server/src/service/ping.rs       # Ping 服务测试
crates/server/src/middleware/auth.rs    # 中间件测试
crates/server/src/test_utils.rs         # 测试辅助 (setup_test_db)
crates/server/tests/integration.rs      # 集成测试
crates/server/tests/docker_integration.rs # Docker 集成测试
crates/agent/src/collector/tests.rs     # Agent 采集器测试
crates/agent/src/collector/disk_io.rs   # Disk I/O 纯函数测试
crates/agent/src/pinger.rs              # Agent Pinger 测试
crates/agent/src/probe_utils.rs         # 批量探测解析测试
crates/agent/src/network_prober.rs      # 网络探测模块测试
crates/agent/src/config.rs              # Agent 配置测试
crates/agent/src/reporter.rs            # Traceroute 测试
crates/agent/src/file_manager.rs        # 文件管理器测试
crates/server/src/presets/mod.rs        # 预设目标加载测试
crates/server/src/service/network_probe.rs # 网络探测服务测试
crates/server/src/service/file_transfer.rs # 文件传输管理器测试
crates/server/src/service/traffic.rs    # 流量统计服务测试
crates/server/src/service/service_monitor.rs # 服务监控测试
crates/server/src/service/checker/dns.rs   # DNS checker 测试
crates/server/src/service/checker/http_keyword.rs # HTTP Keyword checker 测试
crates/server/src/service/checker/ssl.rs   # SSL checker 测试
crates/server/src/service/checker/tcp.rs   # TCP checker 测试
crates/server/src/service/checker/whois.rs # WHOIS checker 测试
crates/server/src/service/dashboard.rs  # Dashboard 测试
crates/server/src/service/uptime.rs     # Uptime 测试
crates/server/src/config.rs             # 配置测试
crates/server/src/router/api/brand.rs   # Brand API 测试
apps/web/src/hooks/use-terminal-ws.test.ts
apps/web/src/lib/capabilities.test.ts
apps/web/src/lib/api-client.test.ts
apps/web/src/lib/utils.test.ts
apps/web/src/lib/ws-client.test.ts
apps/web/src/hooks/use-auth.test.tsx
apps/web/src/hooks/use-api.test.tsx
apps/web/src/hooks/use-realtime-metrics.test.tsx
apps/web/src/hooks/use-servers-ws.test.ts
apps/web/src/lib/disk-io.test.ts
apps/web/src/lib/file-utils.test.ts
apps/web/src/hooks/use-traffic.test.tsx
apps/web/src/components/server/disk-io-chart.test.tsx
apps/web/src/components/server/traffic-card.test.tsx
apps/web/src/components/server/capabilities-dialog.test.tsx
apps/web/src/components/dashboard/dashboard-layout.test.ts
apps/web/src/hooks/use-dashboard-editor.test.tsx
apps/web/src/hooks/use-dashboard.test.tsx
apps/web/src/components/dashboard/widget-renderer.test.tsx
apps/web/src/components/dashboard/dashboard-grid.test.tsx
apps/web/src/components/dashboard/dashboard-editor-view.test.tsx
apps/web/src/components/dashboard/widget-config-dialog.test.tsx
apps/web/src/routes/_authed/-index.test.tsx
apps/web/src/lib/markdown.test.ts
apps/web/src/components/dashboard/uptime-timeline.test.tsx
apps/web/src/components/geoip.test.tsx
apps/web/vitest.config.ts
.github/workflows/ci.yml
```
