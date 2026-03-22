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
cargo test -p serverbee-server          # 服务端单元 + 集成 + dashboard + uptime (223 unit + 36 integration + 4 docker = 263 tests)
cargo test -p serverbee-agent           # Agent 采集器 + Pinger + NetworkProber + FileManager + Traceroute (55 tests)
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
| `agent/collector/` | 8 | 系统信息、指标范围、使用量约束、磁盘 I/O 基线语义、设备过滤、速率计算排序 |
| `agent/pinger.rs` | 2 | TCP 探测（开放/关闭端口） |
| `agent/config.rs` | 1 | IpChangeConfig 默认值（enabled/check_external_ip/interval_secs/external_ip_url） |
| `server/service/audit.rs` | 3 | 审计日志记录、列表、排序 |
| `server/service/config.rs` | 5 | KV 存取、upsert、类型化读写 |
| `server/presets/mod.rs` | 8 | 预设目标加载、ID 唯一性、查找、分组元数据、探测类型校验 |
| `server/service/network_probe.rs` | 13 | 网络探测目标 CRUD、预设目标保护（update/delete 403）、default_target_ids 校验、server targets 分配+校验、探测记录查询 |
| `agent/probe_utils.rs` | 2 | 批量探测结果解析、地址解析 |
| `agent/network_prober.rs` | 2 | 网络探测任务调度、结果上报 |
| `server/service/file_transfer.rs` | 9 | 传输创建/获取、并发限制、过期清理、状态转换、进度更新、临时文件清理 |
| `agent/file_manager.rs` | 24 | 路径校验(root_paths/遍历/deny_patterns/多根/空根)、目录列表(排序/空目录/元数据)、文件读写(base64编解码/大小限制)、删除/创建目录/重命名、上传流程、下载分片 |
| `server/service/traffic.rs` | 21 | 增量计算(正常/重启/单方向重启/零值)、计费周期范围(月/季/年/自定义起始日)、预测算法(正常/早期/无限额)、DB 操作(upsert 累加/状态缓存/日聚合时区)、overview 空服务器、cycle_history 无计费周期、ServerTrafficOverview/CycleTraffic 序列化 |
| `server/service/task_scheduler.rs` | 3 | TaskScheduler 创建、重叠检测、取消活跃运行 |
| `server/task/task_scheduler.rs` | 2 | correlation_id 格式、唯一性 |
| `server/config.rs` | 1 | 时区解析（chrono-tz 验证） |
| `server/service/service_monitor.rs` | 15 | CRUD、记录管理、check_state 更新、清理策略、级联删除、JSON server_ids |
| `server/service/checker/dns.rs` | 3 | 自定义 nameserver、无效 nameserver、系统默认 resolver 构建 |
| `server/service/checker/http_keyword.rs` | 3 | 空 headers、无效 header 值、带自定义 headers 构建 |
| `server/service/checker/ssl.rs` | 3 | 默认端口解析、显式端口解析、config 覆盖端口解析 |
| `server/service/checker/tcp.rs` | 3 | 连接拒绝、连接成功（本机监听）、默认超时值 |
| `server/service/checker/whois.rs` | 8 | 多种日期格式解析、paid-till 格式、ISO 格式、注册商解析、长文本截断 |
| `server/service/dashboard.rs` | 12 | 仪表盘 CRUD、默认仪表盘自动创建/幂等性、widget diff 更新、排序、删除保护（默认/最后一个）、set_default 清除旧值、未知 widget 类型拒绝 |
| `server/service/uptime.rs` | 5 | get_daily_filled 精确天数返回、日期边界验证、部分数据补零、单日查询、90 天查询 |

### 集成测试覆盖

| 测试 | 流程 |
|------|------|
| `test_agent_register_connect_report` | Agent 注册 → WS 连接 → SystemInfo → 指标上报 |
| `test_server_records_api_returns_disk_io_json` | 注册 Agent → 直接保存带 disk_io 的记录 → GET /api/servers/{id}/records 返回 disk_io_json |
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
| `test_service_monitor_crud_and_check` | 创建 TCP 监控 → 列表验证 → 触发检查 → 验证记录 → 删除 |
| `test_traffic_overview_api` | 空 overview → 注册 Agent + 配置 billing → overview 包含服务器 → daily API 验证 |
| `test_traffic_api_returns_data` | 注册 Agent → 查询流量 API → 验证响应结构 |
| `test_server_billing_start_day` | 更新 billing_start_day → 验证持久化和流量 API 反映 |
| `test_dashboard_crud_cycle` | 创建仪表盘 → 列表 → 更新 (widget diff) → 删除 |
| `test_dashboard_default_auto_creates` | GET /dashboards/default → 自动创建默认仪表盘 → 幂等 |
| `test_dashboard_rbac_member_cannot_write` | member 用户 POST/PUT/DELETE → 403 |
| `test_alert_events_endpoint` | 创建告警规则 → GET /alert-events → 验证响应结构 |
| `test_uptime_daily_requires_auth` | 无认证访问 /api/servers/{id}/uptime-daily → 401 |
| `test_uptime_daily_server_not_found` | 认证后访问不存在的 server → 404 |
| `test_uptime_daily_returns_data` | 注册 Agent → days=0 → 400、days=366 → 400、默认 → 200 (90 条零填充) |

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
| `traffic-card.test.tsx` | 1 | TrafficCard 渲染、Today/Monthly tab 切换、hourly/daily BarChart 验证 |
| `disk-io.test.ts` | 3 | disk_io_json 解析、汇总序列、按磁盘补零序列 |
| `disk-io-chart.test.tsx` | 2 | DiskIoChart 汇总/按磁盘切换、空数据返回 null |
| `dashboard-layout.test.ts` | 4 | widgetsToLayout/layoutToPatch/mergeLayoutPatch 转换、normalizeNewWidgetPlacement 处理新 widget 初始落位 |
| `use-dashboard-editor.test.tsx` | 9 | 编辑草稿生命周期、layout patch 合并、add/update/delete/cancel、buildSaveInput 解析 config_json |
| `use-dashboard.test.tsx` | 8 | useDashboards/useDefaultDashboard/useDashboard 查询、useCreateDashboard/useUpdateDashboard/useDeleteDashboard 变更、空 id 守卫、保存成功后同步 dashboard/default cache |
| `widget-renderer.test.tsx` | 14 | 13 种 widget 类型逐一渲染无崩溃（stat-number/server-cards/gauge/top-n/line-chart/multi-line/traffic-bar/disk-io/alert-list/service-status/server-map/markdown/uptime-timeline）+ 未知类型 fallback |
| `dashboard-grid.test.tsx` | 9 | 拖拽/缩放期间仅更新本地 live layout、stop 时才向父层提交 patch、交互中不被外部 rerender 覆盖、移动端单列布局 |
| `dashboard-editor-view.test.tsx` | 7 | Save/Cancel 编排、layout draft 提交、picker+config 添加 widget、编辑已有 widget、dashboard 切换时 reset/flush 本地编辑态、加载间隙保持选中 dashboard id |
| `routes/_authed/index.test.tsx` | 1 | route 在切换 dashboard 且目标数据尚未返回时，仍向 editor view 传递稳定的 activeDashboardId |
| `widget-config-dialog.test.tsx` | 9 | stat-number metric 选择、line-chart server+metric+range 选择、markdown textarea、service-status/server-map 无配置提示、title 输入、编辑模式标题、关闭时不渲染、uptime-timeline server 多选 |
| `markdown.test.ts` | 8 | 标题/粗体斜体/安全链接/javascript:链接拦截/HTML 标签转义/img onerror 转义/行内代码/无序列表 |
| `capabilities-dialog.test.tsx` | 1 | admin 用户触发按钮打开能力控制对话框 |
| `uptime-timeline.test.tsx` | 11 | 分段数量、绿/黄/红/灰颜色逻辑、自定义阈值、标签显示、图例显示、数据补零、computeAggregateUptime null/正常值/100% |

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

## 浏览器自动化测试（agent-browser）

使用 [agent-browser](https://github.com/anthropics/agent-browser) CLI 进行 E2E 浏览器测试。

### 前置条件

```bash
# 安装 agent-browser
npm i -g agent-browser
# 或
brew install agent-browser

# 安装 Chrome（如果没有）
agent-browser install
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

# 4. P9: Service Monitor — 创建 TCP 监控
agent-browser open http://localhost:9527/settings/service-monitors
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser click @e20  # Add Monitor
agent-browser wait 1000 && agent-browser snapshot -i
# 选择 TCP 类型
agent-browser click @e2  # Type dropdown
agent-browser wait 500 && agent-browser snapshot -i
agent-browser click @e7  # TCP option
agent-browser wait 500 && agent-browser snapshot -i
agent-browser fill @e3 "127.0.0.1:9527"  # Target
agent-browser click @e7  # Create
agent-browser wait --load networkidle
# 验证：列表显示新监控项
agent-browser snapshot -i
# 触发检查
agent-browser click @e24  # Trigger check
agent-browser wait 2000
# 查看详情
agent-browser click @e22  # View details
agent-browser wait --load networkidle
agent-browser screenshot /tmp/sm-detail.png

# 5. P10: Traffic — 查看流量总览
agent-browser open http://localhost:9527/traffic
agent-browser wait --load networkidle
agent-browser screenshot /tmp/traffic.png

# 6. P11: IP Changed — 验证告警规则类型
agent-browser open http://localhost:9527/settings/alerts
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser click @e20  # Add
agent-browser wait 1000 && agent-browser snapshot -i
agent-browser click @e26  # Conditions dropdown
agent-browser wait 500 && agent-browser snapshot -i
# 验证："IP Changed" 选项存在于列表末尾

# 7. 清理
agent-browser close
pkill -f "target/debug/serverbee-server"
```

```bash
# 8. P18: Uptime Timeline — 公共状态页时间线
agent-browser open http://localhost:9527/status
agent-browser wait --load networkidle
agent-browser screenshot /tmp/p18-status-simple.png

# 创建状态页（如果还没有的话）
agent-browser open http://localhost:9527/settings/status-pages
agent-browser wait --load networkidle && agent-browser snapshot -i
# 查看状态页有无 uptime 阈值配置字段
agent-browser screenshot /tmp/p18-status-admin.png

# 服务器详情页 — Uptime 卡片
agent-browser open http://localhost:9527/servers
agent-browser wait --load networkidle && agent-browser snapshot -i
# 点击第一台服务器
agent-browser click @e1
agent-browser wait --load networkidle
agent-browser screenshot /tmp/p18-server-detail.png
# 验证：Uptime 卡片显示百分比 + 90 天时间线

# Dashboard — Uptime Timeline Widget
agent-browser open http://localhost:9527
agent-browser wait --load networkidle && agent-browser snapshot -i
agent-browser screenshot /tmp/p18-dashboard.png
```

### 最近一次自动化测试结果（2026-03-19）

| 测试项 | 结果 |
|--------|------|
| Service Monitor 列表页渲染 | ✅ 表格正确显示 Status/Name/Type/Target/Interval/Enabled/LastChecked/Actions |
| 创建 TCP 监控 | ✅ 类型选择→填写 target→创建成功→列表更新 |
| 触发手动检查 | ✅ 绿色状态点→last_checked 更新 |
| 详情页渲染 | ✅ Uptime 100%、Avg Latency 0.3ms、Response Time 面积图、TCP Connection 卡片 |
| Traffic 总览页渲染 | ✅ 4 张统计卡片 + "No servers with traffic data yet" |
| 告警 IP Changed 规则类型 | ✅ 条件下拉包含 "IP Changed" 选项（第 20 种类型） |
| 侧边栏导航 | ✅ Service Monitors 和 Traffic 入口正确 |

## 前端性能测试

### 实时图表性能

Server 详情页 realtime 模式下有 7 个 Recharts 图表随 WebSocket 数据实时更新，是性能热点。

#### 优化措施

| 措施 | 文件 | 说明 |
|------|------|------|
| 渲染节流 | `hooks/use-realtime-metrics.ts` | `RENDER_THROTTLE_MS=2000`，限制最多每 2 秒触发一次 re-render |
| 缩短动画 | `components/server/metrics-chart.tsx` | `animationDuration={800}`（默认 1500ms），避免与 2s 更新周期叠加 |
| 缩短动画 | `components/server/disk-io-chart.tsx` | 同上 |

#### 使用 agent-browser 测量性能

```bash
# 1. 登录并导航到 server 详情 realtime 页面
agent-browser open http://localhost:5173/login
agent-browser snapshot -i
agent-browser fill @e5 "admin" && agent-browser fill @e6 "admin123" && agent-browser click @e4
agent-browser wait --load networkidle
agent-browser open "http://localhost:5173/servers/<SERVER_ID>?range=realtime"
agent-browser wait --load networkidle

# 2. 测量 DOM mutations + Long Tasks（10 秒）
agent-browser eval --stdin <<'EVALEOF'
(function() {
  return new Promise(resolve => {
    let domMutations = 0, longTasks = 0, longTaskDurations = [];
    const observer = new MutationObserver(m => { domMutations += m.length; });
    observer.observe(document.body, { childList: true, subtree: true, attributes: true });
    const perfObserver = new PerformanceObserver(list => {
      for (const e of list.getEntries()) { longTasks++; longTaskDurations.push(Math.round(e.duration)); }
    });
    try { perfObserver.observe({ entryTypes: ['longtask'] }); } catch(e) {}
    setTimeout(() => {
      observer.disconnect(); perfObserver.disconnect();
      resolve(JSON.stringify({ domMutations, longTasks, longTaskDurations }));
    }, 10000);
  });
})()
EVALEOF

# 3. 测量 FPS（10 秒）
agent-browser eval --stdin <<'EVALEOF'
(function() {
  return new Promise(resolve => {
    let frames = 0, lastTime = performance.now(), fpsReadings = [];
    function tick() {
      frames++;
      const now = performance.now();
      if (now - lastTime >= 1000) {
        fpsReadings.push(Math.round(frames * 1000 / (now - lastTime)));
        frames = 0; lastTime = now;
      }
      if (fpsReadings.length < 10) requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
    setTimeout(() => {
      const avg = fpsReadings.length ? Math.round(fpsReadings.reduce((a,b) => a+b, 0) / fpsReadings.length) : 0;
      resolve(JSON.stringify({ avgFps: avg, minFps: Math.min(...fpsReadings), maxFps: Math.max(...fpsReadings), fpsPerSecond: fpsReadings }));
    }, 10500);
  });
})()
EVALEOF

# 4. 清理
agent-browser close
```

#### 性能基准（2026-03-21）

测试环境：Server 详情页 realtime 模式，7 个图表，1920×963 视口

| 指标 | 数值 | 阈值 |
|------|------|------|
| DOM mutations / 10s | 2469 | < 5000 |
| Long tasks | 3 次（68/72/69ms） | 每个 < 100ms |
| 平均 FPS | 62 | > 50 |
| 最低 FPS | 50 | > 30 |
| 内存 (JS Heap) | 37→50 MB / 10s | < 200 MB |

#### 性能回归判断标准

- **FPS 平均值 < 30**：需要优化
- **Long task > 200ms**：需要排查
- **DOM mutations / 10s > 10000**：可能有动画叠加或缺少节流

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
| Dashboard | `/` | 自定义仪表盘：默认加载 default dashboard（6 个预设 widget），支持切换/新建/删除仪表盘，编辑模式拖拽布局 + 添加/配置/删除 widget，13 种 widget 类型（含 Uptime Timeline） | — |
| Servers 列表 | `/servers` | 表格显示服务器，支持搜索、排序、批量选择 | ✅ |
| 服务器详情 | `/servers/:id` | 系统信息（OS/CPU/RAM/Kernel）、Uptime 卡片（90 天百分比 + 时间线）、实时流式图表（默认）+ 历史图表（1h/6h/24h/7d/30d）、CPU/Memory/Disk/Network In/Out/Load/Temperature，历史模式额外显示 Disk I/O（汇总/按磁盘） | — |
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
| 服务监控 | `/settings/service-monitors` | 监控列表 + 创建/编辑/删除/手动触发 + 状态徽章 + 侧边栏导航 | ✅ |
| 服务监控详情 | `/service-monitors/:id` | Uptime%/延迟/最后检测统计卡片 + Response Time 面积图 + 类型详情卡片 + 历史记录 | ✅ |
| 网络质量总览 | `/network` | 显示 VPS 网络质量卡片列表，统计栏显示总数/在线/异常 | — |
| 网络质量详情 | `/network/:id` | 目标卡片 + 多线延迟图表 + 异常摘要 + 底部统计 + CSV 导出 | — |
| 网络探测设置 | `/settings/network-probes` | 目标管理（96 预设 + 自定义 CRUD）+ 全局设置（间隔/包数/默认目标） | — |
| Docker 监控 | `/servers/:serverId/docker` | 概览卡片 + 容器表格 + 事件时间线 + 详情弹窗 + 网络/卷弹窗 | — |
| 流量总览 | `/traffic` | 统计卡片（周期 In/Out/最高用量/超限数）+ 服务器排名表格 + 30 天趋势 AreaChart | ✅ |
| 服务器流量 Tab | `/servers/:id` (Traffic tab) | 周期进度条 + 日 BarChart (7d/30d/90d) + 历史周期对比水平 BarChart | — |
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

### 验证清单 — 磁盘 I/O 监控

#### 单元测试覆盖（自动化）

以下自动化测试覆盖位于 Rust agent/server、协议层和前端图表层，可分别通过 `cargo test --workspace` 与 `bun run test` 运行：

| 测试组 | 文件 / 测试名 | 验证内容 |
|--------|---------------|----------|
| 协议兼容 | `crates/common/src/types.rs` / `test_system_report_without_disk_io_defaults_to_none` | 旧 payload 缺少 `disk_io` 字段时仍能反序列化，向后兼容为 `None` |
| 协议 round-trip | `crates/common/src/protocol.rs` / `test_report_with_disk_io_round_trip` | `AgentMessage::Report` 序列化/反序列化后保留 `disk_io` 数据 |
| Agent 采集语义 | `crates/agent/src/collector/tests.rs` / `test_collect_disk_io_first_sample_is_empty`、`test_collect_disk_io_is_none_on_unsupported_platforms` | Linux 首次采样返回空数组建立基线；非 Linux 平台返回 `None` |
| Agent 纯函数 | `crates/agent/src/collector/disk_io.rs` / `test_compute_disk_io_sorts_devices_and_clamps_negative_deltas`、`test_should_track_device_filters_virtual_and_partition_names` | 速率计算、设备名排序、计数器回退钳制、虚拟/分区设备过滤 |
| Server 持久化 | `crates/server/src/service/record.rs` / `test_save_report_persists_disk_io_json`、`test_aggregate_hourly_averages_disk_io_by_device` | `disk_io_json` 原始记录持久化，以及小时聚合时按设备求平均 |
| 前端数据转换 | `apps/web/src/lib/disk-io.test.ts` / `parseDiskIoJson`、`buildMergedDiskIoSeries`、`buildPerDiskIoSeries` | JSON 解析容错、汇总序列构建、按磁盘补零与稳定排序 |
| 前端图表渲染 | `apps/web/src/components/server/disk-io-chart.test.tsx` / `renders merged and per-disk views`、`returns null when there is no disk I/O data` | `Merged` / `Per Disk` 视图切换、空数据时不渲染卡片 |

#### 集成测试覆盖（自动化）

以下集成测试位于 `crates/server/tests/integration.rs`，启动真实 Server + SQLite 临时数据库：

| 测试名 | 流程 |
|--------|------|
| `test_server_records_api_returns_disk_io_json` | 注册 Agent → 直接保存带 `disk_io` 的记录 → `GET /api/servers/{id}/records` 返回 `disk_io_json`，且 JSON 可反序列化为按磁盘读写速率 |

#### E2E 手动验证

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| DI1 | 实时模式隐藏 Disk I/O | 打开 `/servers/:id` → 默认保持 `Real-time` 模式 → 页面中不显示 Disk I/O 卡片 | — |
| DI2 | 历史模式显示 Disk I/O | 点击 `1h`（或 `6h/24h/7d/30d`）→ 等待历史 records 加载 → 显示 `Disk I/O` 卡片与 `Merged` / `Per Disk` tabs | — |
| DI3 | 汇总视图双线 | 保持 `Merged` tab → hover 图表 → Tooltip 显示 `Read` / `Write` 两条线，速率按 `KB/s` / `MB/s` / `GB/s` 格式化 | — |
| DI4 | 按磁盘视图 | 点击 `Per Disk` → 每块磁盘单独渲染图表，标题按设备名排序（如 `sda`、`sdb`、`nvme0n1`） | — |
| DI5 | 缺失时间点补零 | 构造某个时间点仅部分磁盘有数据 → 切到 `Per Disk` → 缺失磁盘的该时间点显示为 0，不报错不丢线 | — |
| DI6 | 时间范围切换 | 依次点击 `1h/6h/24h/7d/30d` → Disk I/O 图表的时间轴和数据范围同步更新，不串用其他范围的数据 | — |
| DI7 | 零吞吐历史可见 | 准备 read/write 全为 0 的历史记录 → 切到历史模式 → Disk I/O 卡片仍可见，图表显示空闲基线而非整卡消失 | — |
| DI8 | 旧 Agent / 非 Linux 兼容 | 接入旧 agent 或非 Linux agent → 切到历史模式 → 页面不报错，Disk I/O 区域按无数据处理 | — |
| DI9 | API 返回原始 JSON | 调用 `GET /api/servers/{id}/records?interval=raw` → 响应中包含 `disk_io_json`，内容可反序列化为每磁盘 `read_bytes_per_sec` / `write_bytes_per_sec` | — |
| DI10 | i18n | 切换中文/英文 → `磁盘 I/O` / `Disk I/O`、`汇总` / `Merged`、`按磁盘` / `Per Disk`、`读取` / `Read`、`写入` / `Write` 文案正确 | — |

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
| **总览/历史** | `test_overview_empty` | 无服务器 → overview 返回空 vec |
| | `test_cycle_history_no_billing_cycle` | 服务器无 billing_cycle → overview 不包含 + cycle_history 返回零流量 |
| **序列化** | `test_server_traffic_overview_serialization` | ServerTrafficOverview JSON 字段完整性 |
| | `test_cycle_traffic_serialization` | CycleTraffic JSON 字段完整性 |

#### 集成测试覆盖（自动化）

以下集成测试位于 `crates/server/tests/integration.rs`，启动真实 Server + SQLite 临时数据库：

| 测试名 | 流程 |
|--------|------|
| `test_oneshot_task_backward_compat` | migration 新增 NOT NULL 列（带默认值）后 → 一次性任务仍可正常创建 |
| `test_service_monitor_crud_and_check` | 创建 TCP 监控（目标为测试服务器端口）→ 列表验证 → 触发检查 → TCP 连接成功 → 记录创建 → 删除清理 |
| `test_traffic_overview_api` | 空 overview 返回空数组 → 注册 Agent + 配置 billing_cycle → overview 包含服务器 + 验证字段 → daily API 有效 |
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
| T23 | 流量总览排行 | /traffic → 表格按用量排序 → 最高用量服务器排第一 | — |
| T24 | 流量总览趋势图 | /traffic → 30 天趋势 AreaChart 显示入站/出站两条线 | — |
| T25 | 服务器 Traffic Tab | 设置 billing_cycle → /servers/:id → Traffic Tab 可见 → 周期进度条 + 日柱图 + 历史对比 | — |

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
| SM2 | 创建 TCP 监控 | 类型 TCP → 输入 host:port → 创建 → 状态显示 OK/FAIL | ✅ |
| SM3 | 创建 HTTP Keyword 监控 | 类型 HTTP → 输入 URL + 关键词 → 创建 → 检测响应体含关键词 | — |
| SM4 | 创建 DNS 监控 | 类型 DNS → 输入域名 + 期望 IP → 创建 → 解析结果与预期匹配则 OK | — |
| SM5 | 创建 WHOIS 监控 | 类型 WHOIS → 输入域名 → 创建 → 显示到期剩余天数 | — |
| SM6 | 手动触发检测 | 点击 Check Now → toast 提示 → 状态和最后检测时间更新 | ✅ |
| SM7 | 编辑监控 | 点击 Edit → 修改名称/阈值 → 保存 → 列表更新 | — |
| SM8 | 删除监控 | 点击 Delete → 确认 → 从列表消失 → 历史记录级联删除 | — |
| SM9 | 状态徽章 | OK = 绿色，FAIL = 红色，PENDING = 灰色 | ✅ |
| SM10 | 详情页 — 状态图表 | 点击监控 → 详情页 → 显示 Response Time 面积图 + Uptime/延迟/最后检测统计 | ✅ |
| SM11 | 详情页 — 记录表格 | 详情页显示历史检测记录（时间/状态/延迟/消息）+ 时间范围过滤 | — |
| SM12 | 自动调度 | 创建监控 → 等待 interval 秒 → 自动执行 → 历史记录出现新条目 | — |
| SM13 | SSL 到期警告 | SSL 证书剩余天数 < threshold → 状态变为 FAIL | — |
| SM14 | WHOIS 到期警告 | 域名到期剩余天数 < threshold → 状态变为 FAIL | — |
| SM15 | 记录清理 | 配置 service_monitor_record_days=1 → cleanup 运行后旧记录删除 | — |
| SM16 | i18n 中文 | 切换中文 → 页面标题/按钮/状态标签全部显示中文 | — |
| SM17 | i18n 英文 | 切换英文 → 所有 UI 元素显示英文 | — |

### 验证清单 — IP 变更通知

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| IP1 | 被动检测 — remote_addr 变更 | Agent 断线 → 从不同 IP 重连 → 审计日志出现 ip_changed 记录 | — |
| IP2 | 被动检测 — last_remote_addr 更新 | Agent 连接 → GET /api/servers/:id → last_remote_addr 字段有值 | — |
| IP3 | 主动检测 — NIC 变更 | Agent 运行中 → 添加/移除网络接口 → 5 分钟内检测到变更 | — |
| IP4 | 主动检测 — 外部 IP (可选) | 配置 check_external_ip=true → 公网 IP 变化时上报 | — |
| IP5 | 事件驱动告警 | 创建 ip_changed 告警规则 → 关联通知组 → IP 变更时触发通知 | — |
| IP6 | 告警规则覆盖范围 | 创建 cover_type=include 规则 → 仅指定服务器触发 | — |
| IP7 | Browser 推送 | Dashboard 打开时 → IP 变更 → WS 推送 ServerIpChanged 消息 | — |
| IP8 | GeoIP 更新 | IP 变更后 → 服务器 region/country_code 自动更新 | — |
| IP9 | 配置禁用 | 设置 ip_change.enabled=false → Agent 不发送 IpChanged | — |
| IP10 | i18n | 切换中英文 → 告警规则类型 "IP Changed"/"IP 变更" 正确显示 | — |

### 验证清单 — 自定义仪表盘

#### 仪表盘 CRUD

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| DB1 | 默认仪表盘自动创建 | 首次登录 → `/` 自动创建默认 Dashboard → 显示 6 个预设 widget（5 stat-number + 1 server-cards） | — |
| DB2 | 默认仪表盘幂等 | 刷新页面 → 仍显示相同仪表盘 ID 和 6 个 widget | — |
| DB3 | 新建仪表盘 | 点击 `+ New` → 输入名称 → 创建 → 切换到新仪表盘（空白） | — |
| DB4 | 切换仪表盘 | 下拉选择另一个仪表盘 → 页面加载对应 widget 布局 | — |
| DB5 | 设为默认 | 选择非默认仪表盘 → 点击 Set Default（星号按钮）→ 刷新后默认加载该仪表盘 | — |
| DB6 | 删除仪表盘 | 选择非默认仪表盘 → 点击 Delete → 确认 → 切换回其他仪表盘 | — |
| DB7 | 删除默认仪表盘保护 | 默认仪表盘的 Delete 按钮不显示或禁用 | — |
| DB8 | RBAC — Member 只读 | Member 用户登录 → 不显示 Edit/New/Delete 按钮 → 仅可查看 | — |

#### 编辑模式

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| DB9 | 进入编辑模式 | 点击 Edit 按钮 → widget 可拖拽/调整大小 → 显示 Save/Cancel 按钮 | — |
| DB10 | 拖拽布局 | 编辑模式下拖拽 widget 到新位置 → 释放后布局更新 | — |
| DB11 | 调整大小 | 编辑模式下拖拽 widget 右下角 → 尺寸变化 | — |
| DB12 | 添加 Widget | 编辑模式 → 点击 Add Widget → 弹出 Widget Picker → 选择类型 → 弹出配置对话框 → 填写 → 确认 → widget 出现在画布 | — |
| DB13 | 编辑 Widget 配置 | 编辑模式 → hover widget → 点击铅笔图标 → 修改配置 → 确认 → widget 更新 | — |
| DB14 | 删除 Widget | 编辑模式 → hover widget → 点击垃圾桶图标 → widget 移除 | — |
| DB15 | 保存布局 | 编辑后点击 Save → PUT /api/dashboards/:id → 刷新后布局保持 | — |
| DB16 | 取消编辑 | 编辑后点击 Cancel → 所有修改丢弃 → 恢复原布局 | — |

#### Widget 类型渲染

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| DB17 | stat-number | 添加 stat-number widget → 选择 metric（server_count/avg_cpu/avg_memory/total_bandwidth/health）→ 显示图标 + 数值 + 标签 | — |
| DB18 | server-cards | 添加 server-cards widget → 显示服务器卡片网格 → 实时更新 | — |
| DB19 | gauge | 添加 gauge widget → 选择服务器 + 指标 → 显示径向进度条 + 百分比 | — |
| DB20 | top-n | 添加 top-n widget → 选择指标 + 数量 → 显示排名列表 + 进度条 | — |
| DB21 | line-chart | 添加 line-chart widget → 选择服务器 + 指标 + 时间范围 → 显示历史折线图 | — |
| DB22 | multi-line | 添加 multi-line widget → 选择多台服务器 + 指标 → 显示多线对比图 + 图例 | — |
| DB23 | traffic-bar | 添加 traffic-bar widget → 可选服务器 → 显示 in/out 堆叠柱状图 | — |
| DB24 | disk-io | 添加 disk-io widget → 选择服务器 → 显示磁盘读写折线图 | — |
| DB25 | alert-list | 添加 alert-list widget → 显示告警事件列表（红/绿状态点 + 规则名 + 服务器 + 相对时间） | — |
| DB26 | service-status | 添加 service-status widget → 显示服务监控点阵（绿/黄/红/灰圆点）→ hover 显示监控名 + 状态 | — |
| DB27 | server-map | 添加 server-map widget → SVG 世界地图高亮有服务器的国家 → 圆形标记在国家质心 | — |
| DB28 | markdown | 添加 markdown widget → 输入 Markdown 内容 → 渲染标题/粗体/链接/列表 → 无 XSS（`<script>` 被转义） | — |

#### 响应式 & 移动端

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| DB29 | 移动端布局 | 窗口宽度 < 768px → widget 按 sort_order 垂直排列（非 grid 布局） | — |
| DB30 | 桌面端布局 | 窗口宽度 ≥ 768px → 12 列 grid 布局 + 拖拽调整 | — |

#### API 验证

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| DB31 | GET /api/dashboards | 返回所有仪表盘列表，按 sort_order 排序 | — |
| DB32 | GET /api/dashboards/default | 首次调用自动创建 → 返回 6 个 widget → 第二次返回相同 ID | — |
| DB33 | POST /api/dashboards | 创建新仪表盘 → 返回 dashboard model | — |
| DB34 | PUT /api/dashboards/:id | 更新名称/widget diff（增/改/删）→ 返回完整 DashboardWithWidgets | — |
| DB35 | DELETE /api/dashboards/:id | 删除非默认仪表盘 → 200 → 删除默认仪表盘 → 400 | — |
| DB36 | GET /api/alert-events | 返回聚合告警事件列表，firing 在前 → 支持 limit 参数 | — |
| DB37 | OpenAPI | `/swagger-ui/` 包含 6 个 dashboards + 1 个 alert-events 端点 | — |

### 验证清单 — Uptime 90 天时间线 (P18)

#### 公共状态页

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| UT1 | 时间线替换进度条 | 打开 `/status/:slug` → 每台服务器行显示 90 段色块时间线（替代旧进度条） | — |
| UT2 | Hover Tooltip | 鼠标悬停色块 → 显示日期、在线率、在线时长、宕机次数 | — |
| UT3 | 百分比显示 | 每行末尾显示聚合在线率百分比（或"—"表示无数据） | — |
| UT4 | 阈值颜色 | 100% 日显示绿色、低于阈值显示黄色/红色、无数据显示灰色 | — |
| UT5 | 移动端隐藏 | 窗口宽度 < 640px → 时间线隐藏，仅显示百分比文字 | — |

#### 服务器详情页

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| UT6 | Uptime 卡片 | 打开 `/servers/:id` → 指标区域上方显示 Uptime 卡片（大字百分比 + 时间线 + 标签 + 图例） | — |
| UT7 | 无数据显示 | 新注册服务器无 uptime 数据 → 显示"—"和全灰时间线 | — |

#### 仪表盘 Widget

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| UT8 | 添加 Widget | 编辑模式 → Add Widget → 选择 "Uptime Timeline" → 选择服务器 + 天数 → 添加 | — |
| UT9 | 单服务器 | 配置 1 台服务器 → 显示名称 + 百分比 + 完整时间线 | — |
| UT10 | 多服务器 | 配置多台服务器 → 垂直堆叠每行一台，较矮时间线(20px) | — |
| UT11 | 天数选择 | 30/60/90 天选项 → 时间线段数对应变化 | — |

#### 状态页管理 — 阈值配置

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| UT12 | 创建时配置阈值 | 创建状态页 → 黄色阈值 99.9%、红色阈值 95% → 创建成功 | — |
| UT13 | 编辑时读取已保存值 | 编辑已有状态页 → 阈值输入框显示已保存的值 | — |
| UT14 | 默认值 | 不修改阈值 → 使用默认值 100/95 | — |

#### API 验证

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| UT15 | GET /api/servers/:id/uptime-daily | 认证后请求 → 200，返回 90 条 entries | — |
| UT16 | 404 不存在的服务器 | 请求不存在 ID → 404 | — |
| UT17 | 401 无认证 | 无 session 请求 → 401 | — |
| UT18 | days 参数校验 | days=0 → 400、days=366 → 400、days=30 → 200 (30 条) | — |
| UT19 | 状态页 API 包含 daily | GET /api/status/:slug → servers 数组每项包含 uptime_daily 数组 | — |
| UT20 | serde rename 对齐 | 响应字段名为 server_id/server_name/uptime_percent（非 id/name/uptime_percentage） | — |

#### i18n

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| DB38 | 中文模式 | 切换中文 → 编辑/保存/取消/添加组件/新建仪表盘/删除 等按钮显示中文 | — |
| DB39 | 英文模式 | 切换英文 → Edit/Save/Cancel/Add Widget/New Dashboard/Delete 显示英文 | — |
| DB40 | Widget Picker 中文 | Widget 选择面板中 12 种类型的名称和描述显示中文 | — |

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

### 验证清单 — 三网 Ping + Traceroute (P13)

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| TR1 | By Provider 视图 | `/network/:serverId` → 点击 "By Provider" tab → 显示 CT/CU/CM/International 分组 | — |
| TR2 | Provider 延迟统计 | By Provider 视图 → 每个 provider 显示平均延迟和丢包率 | — |
| TR3 | Traceroute 触发 | `/network/:serverId` → 输入 IP → 点击 "Run Traceroute" → 显示加载状态 | — |
| TR4 | Traceroute 结果 | 等待完成 → 表格显示 Hop/IP/RTT1/RTT2/RTT3 → 延迟色彩编码 | — |
| TR5 | Traceroute 错误 | 输入无效目标 → 显示错误消息 | — |
| TR6 | 能力校验 | 禁用 CAP_PING_ICMP → traceroute 请求 → 403 | — |

### 验证清单 — 多主题 + 品牌定制 (P14)

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| TH1 | 主题切换 | `/settings/appearance` → 点击 Tokyo Night → 全站颜色立即变化 | — |
| TH2 | 主题持久化 | 选择 Nord → 刷新页面 → 仍为 Nord 主题 | — |
| TH3 | 深/浅模式兼容 | 选择 Catppuccin → 切换深/浅模式 → 两种模式都有正确配色 | — |
| TH4 | 默认主题恢复 | 选择 Default → 恢复原始配色 | — |
| TH5 | Logo 上传 | 品牌设置 → 上传 PNG logo → 预览显示 → 保存 → 侧边栏 logo 更新 | — |
| TH6 | Favicon 上传 | 上传 favicon → 保存 → 浏览器标签 favicon 更新 | — |
| TH7 | 标题修改 | 修改 site_title → 保存 → 侧边栏标题更新 | — |
| TH8 | 文件类型限制 | 上传 SVG 文件 → 被拒绝（仅 PNG/ICO） | — |
| TH9 | 大小限制 | 上传 >512KB 文件 → 被拒绝 | — |

### 验证清单 — 状态页增强 (P15)

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| SP1 | 创建状态页 | `/settings/status-pages` → Status Pages tab → Create → 填写标题/slug → 创建 | — |
| SP2 | 公开访问 | 未登录 → 访问 `/status/:slug` → 显示状态页 | — |
| SP3 | 全局状态横幅 | 所有服务器在线 → 绿色 "All Systems Operational" | — |
| SP4 | 创建事件 | Incidents tab → Create → 填写标题/严重程度 → 创建 | — |
| SP5 | 添加事件更新 | 点击事件 → 添加状态更新（investigating → identified → resolved） | — |
| SP6 | 事件在状态页显示 | 公开状态页显示活跃事件 + 更新时间线 | — |
| SP7 | 创建维护窗口 | Maintenance tab → Create → 设置开始/结束时间 → 创建 | — |
| SP8 | 维护告警静默 | 服务器在维护窗口内 → 告警不触发通知 | — |
| SP9 | 维护在状态页显示 | 公开状态页显示计划维护通知 | — |
| SP10 | 删除状态页 | 删除 → 公开访问返回 404 | — |

### 验证清单 — 移动端响应式 + PWA (P16)

| # | 测试场景 | 操作步骤 | 状态 |
|---|---------|---------|------|
| RW1 | 侧边栏抽屉 | 缩小窗口 <1024px → 侧边栏隐藏 → 点击汉堡菜单 → 侧边栏从左滑入 | — |
| RW2 | 抽屉导航 | 点击侧边栏链接 → 导航到目标页 → 抽屉自动关闭 | — |
| RW3 | Dashboard 响应式 | 缩小窗口 → 服务器卡片从 4 列变 2 列变 1 列 | — |
| RW4 | Dialog 全屏 | 缩小窗口 <640px → 打开对话框 → 对话框全屏显示 | — |
| RW5 | PWA 安装 | Chrome → 地址栏出现安装提示 → 安装 → 独立窗口打开 | — |
| RW6 | 离线 Shell | 断网 → 页面骨架仍可显示（Service Worker 缓存 shell） | — |

## 测试文件位置

```
crates/common/src/constants.rs          # 能力常量测试（含 CAP_DOCKER）
crates/common/src/protocol.rs           # 协议序列化测试（含 Docker 消息变体 + Report.disk_io round-trip）
crates/common/src/docker_types.rs      # Docker 数据结构序列化测试
crates/common/src/types.rs             # SystemInfo features 字段 + SystemReport.disk_io 向后兼容测试
crates/server/src/service/alert.rs      # 告警服务测试
crates/server/src/service/auth.rs       # 认证服务测试 (含 DB 集成)
crates/server/src/service/notification.rs # 通知服务测试
crates/server/src/service/record.rs     # 记录服务测试（含 disk_io_json 持久化 / 小时聚合）
crates/server/src/service/agent_manager.rs # AgentManager 单元测试 (含 per-entry TTL)
crates/server/src/service/task_scheduler.rs # TaskScheduler 单元测试 (3 tests)
crates/server/src/task/task_scheduler.rs   # 定时任务执行流程测试 (2 tests)
crates/server/src/service/docker_viewer.rs # DockerViewerTracker 单元测试
crates/server/src/service/server.rs     # 服务器 CRUD 测试
crates/server/src/service/user.rs       # 用户服务测试
crates/server/src/service/ping.rs       # Ping 服务测试
crates/server/src/middleware/auth.rs    # 中间件 Cookie/Key 提取测试
crates/server/src/test_utils.rs         # 测试辅助 (setup_test_db)
crates/server/tests/integration.rs      # 集成测试 (33 tests)
crates/server/tests/docker_integration.rs # Docker 集成测试 (4 tests)
crates/agent/src/collector/tests.rs     # Agent 采集器测试（含 Disk I/O 首次采样 / 平台语义）
crates/agent/src/collector/disk_io.rs   # Disk I/O 纯函数测试（速率计算 / 设备过滤）
crates/agent/src/pinger.rs              # Agent Pinger 测试
crates/agent/src/probe_utils.rs         # 批量探测解析测试
crates/agent/src/network_prober.rs      # 网络探测模块测试
crates/server/src/presets/mod.rs            # 预设目标加载测试
crates/server/src/service/network_probe.rs # 网络探测服务单元测试
crates/server/src/service/file_transfer.rs # 文件传输管理器测试 (9 tests)
crates/server/src/service/traffic.rs       # 流量统计服务测试 (21 tests)
crates/server/src/config.rs                # 配置测试 (1 test)
crates/agent/src/config.rs                # Agent IpChangeConfig 默认值测试 (1 test)
crates/agent/src/reporter.rs              # Traceroute 目标验证 + 输出解析测试 (4 tests)
crates/server/src/router/api/brand.rs     # Brand API magic bytes + 配置测试 (4 tests)
crates/agent/src/file_manager.rs           # Agent 文件管理器测试 (24 tests)
crates/server/src/service/service_monitor.rs # 服务监控 CRUD + 记录管理测试 (15 tests)
crates/server/src/service/checker/dns.rs   # DNS checker 单元测试 (3 tests)
crates/server/src/service/checker/http_keyword.rs # HTTP Keyword checker 单元测试 (3 tests)
crates/server/src/service/checker/ssl.rs   # SSL checker 单元测试 (3 tests)
crates/server/src/service/checker/tcp.rs   # TCP checker 单元测试 (3 tests)
crates/server/src/service/checker/whois.rs # WHOIS checker 单元测试 (8 tests)
crates/server/src/service/dashboard.rs    # Dashboard CRUD + widget diff + default 管理测试 (12 tests)
apps/web/src/hooks/use-terminal-ws.test.ts # Terminal WS hook 测试
apps/web/src/lib/capabilities.test.ts   # 能力位测试
apps/web/src/lib/api-client.test.ts     # API Client 测试
apps/web/src/lib/utils.test.ts          # 工具函数测试
apps/web/src/lib/ws-client.test.ts      # WebSocket Client 测试
apps/web/src/hooks/use-auth.test.tsx    # Auth hook 测试
apps/web/src/hooks/use-api.test.tsx     # API hook 测试
apps/web/src/hooks/use-realtime-metrics.test.tsx # 实时指标 hook 测试（纯函数 + renderHook 集成）
apps/web/src/hooks/use-servers-ws.test.ts # WS 数据合并测试
apps/web/src/lib/disk-io.test.ts       # Disk I/O 历史数据解析 / 序列构建测试
apps/web/src/lib/file-utils.test.ts     # 文件工具函数测试
apps/web/src/hooks/use-traffic.test.tsx # 流量 hook 测试 (3 tests)
apps/web/src/components/server/disk-io-chart.test.tsx # Disk I/O 图表渲染测试
apps/web/src/components/server/traffic-card.test.tsx # TrafficCard tab 切换测试 (1 test)
apps/web/src/components/server/capabilities-dialog.test.tsx # 能力控制对话框测试 (1 test)
apps/web/src/components/dashboard/dashboard-layout.test.ts # Dashboard layout 纯函数测试 (4 tests)
apps/web/src/hooks/use-dashboard-editor.test.tsx     # Dashboard editor hook 测试 (9 tests)
apps/web/src/hooks/use-dashboard.test.tsx            # Dashboard hooks 测试 (8 tests)
apps/web/src/components/dashboard/widget-renderer.test.tsx # Widget 渲染器测试 (14 tests)
apps/web/src/components/dashboard/dashboard-grid.test.tsx  # Dashboard Grid 交互测试 (9 tests)
apps/web/src/components/dashboard/dashboard-editor-view.test.tsx # DashboardEditorView 编排测试 (7 tests)
apps/web/src/components/dashboard/widget-config-dialog.test.tsx # Widget 配置对话框测试 (9 tests)
apps/web/src/routes/_authed/index.test.tsx           # Dashboard route 选中态保持测试 (1 test)
apps/web/src/lib/markdown.test.ts                    # Markdown→HTML 转换器 + XSS 防护测试 (8 tests)
apps/web/vitest.config.ts               # Vitest 配置
.github/workflows/ci.yml               # CI 流水线
```
