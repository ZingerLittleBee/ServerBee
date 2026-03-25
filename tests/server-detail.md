# 服务器列表与详情页测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

### 自动化测试覆盖

#### 前端单元测试

通过 `bun run test` 运行（220 tests passing）：

| 测试组 | 文件 | 验证内容 |
|--------|------|----------|
| API Hooks | `src/hooks/use-api.test.tsx` | `useServer` 获取服务器详情、空 ID 不请求；`useServerRecords` 获取历史记录、enabled=false 不请求 |
| Capabilities | `src/components/server/capabilities-dialog.test.tsx` | Admin 用户可打开 Capability 对话框 |
| Disk I/O | `src/components/server/disk-io-chart.test.tsx` | Merged/Per Disk 视图切换、空数据时返回 null |
| Disk I/O 数据 | `src/lib/disk-io.test.ts` | JSON 解析容错、汇总序列构建、按磁盘补零与稳定排序 |
| Traffic Card | `src/components/server/traffic-card.test.tsx` | Today/Monthly 标签页切换、小时/日柱状图渲染 |

#### 后端单元测试

通过 `cargo test --workspace` 运行：

| 测试组 | 文件 | 验证内容 |
|--------|------|----------|
| 记录持久化 | `crates/server/src/service/record.rs` | `save_report` 保存 disk_io_json、小时聚合按设备求平均 |
| 流量计算 | `crates/server/src/service/traffic.rs` | 增量计算、计费周期范围、预测算法、时区聚合 |
| 协议兼容 | `crates/common/src/types.rs` | 旧 payload 缺 disk_io 仍可反序列化 |
| 集成测试 | `crates/server/tests/integration.rs` | records API 返回 disk_io_json、traffic API、billing_start_day |

---

## 一、服务器列表页（/servers）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-1 | 列表页渲染 | 登录后导航到 `/servers` | 表格显示服务器，支持搜索、排序、批量选择 | ✅ |
| SV-2 | 搜索匹配 | 在搜索框输入服务器名称部分 | 表格显示匹配的服务器 | ✅ |
| SV-3 | 搜索无匹配 | 输入不存在的名称 | 表格为空 | ✅ |
| SV-4 | 编辑对话框 | 点击 Edit | 弹出对话框含 BASIC + BILLING 字段 | ✅ |

---

## 二、服务器详情页（/servers/:id）

### 2.1 页面导航与加载

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-5 | 返回链接 | 进入服务器详情页 → 点击 "Back to Dashboard" 链接 | 导航回到 `/` Dashboard 页面 | ✅ |
| SV-6 | 加载骨架屏 | 首次加载详情页 | 显示骨架屏（Skeleton h-8 w-48 + h-4 w-96 + 两个 h-64） | ⏭️ 本地加载过快 |
| SV-7 | 不存在的服务器 | 访问 `/servers/non-existent-id` | 显示居中文字 "Server not found" | ✅ |

### 2.2 服务器信息区

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-8 | 服务器名称与状态 | 进入在线服务器详情页 | 显示服务器名称（h1 粗体 text-2xl）+ 绿色 "Online" StatusBadge | ✅ |
| SV-9 | 国旗 Emoji | 服务器有 country_code | 名称左侧显示对应国旗 emoji | ⏭️ 本地服务器无 country_code |
| SV-10 | 系统信息栏 | 查看名称下方 | 条件显示：OS、CPU（含核心数和架构）、RAM（格式化 formatBytes）、IPv4、IPv6、Kernel、Region、Agent 版本 | ✅ 显示 OS: macOS 26.3.1, CPU: Apple M1 Pro (10 cores) aarch64, RAM: 32.0 GB, IPv4/IPv6, Kernel, Agent v0.7.1 |
| SV-11 | 离线状态徽章 | Agent 离线时 | StatusBadge 显示红色 "Offline" | ✅ 深色主题切换时捕获到 Offline 状态 |

### 2.3 操作按钮区

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-12 | Edit 按钮 | 点击 Edit 按钮 | 弹出 ServerEditDialog 对话框 | ✅ |
| SV-13 | Capabilities 按钮 | 点击 Capabilities 按钮（Shield 图标） | 弹出 CapabilitiesDialog 对话框，分为 High Risk 和 Low Risk 两组 | ✅ |
| SV-14 | Terminal 按钮 | 服务器在线 + CAP_TERMINAL 启用 | Terminal 按钮可见，点击导航到 `/terminal/$serverId` | ⏭️ CAP_TERMINAL 默认关闭 |
| SV-15 | Files 按钮 | 服务器在线 + CAP_FILE 启用 | Files 按钮可见，点击导航到 `/files/$serverId` | ⏭️ CAP_FILE 默认关闭 |
| SV-16 | Docker 按钮 | 服务器在线 + CAP_DOCKER 启用 | Docker 按钮可见，点击导航到 `/servers/$serverId/docker` | ⏭️ CAP_DOCKER 默认关闭 |
| SV-17 | 离线隐藏按钮 | 服务器离线 | Terminal / Files / Docker 按钮不显示 | ✅ 深色主题截图中确认离线时无这些按钮 |

### 2.4 Uptime 卡片

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-18 | Uptime 卡片渲染 | 查看详情页 | 显示 "Uptime (90 days)" 标题 + 百分比数值 + 90 段色块时间线 + 标签("90 days ago"/"Today") + 图例(Operational/Degraded/Down/No data) | ✅ 90 段色块（89 gray + 1 red），0.00%，标签和图例齐全 |
| SV-19 | 无 Uptime 数据 | 新注册服务器 | Uptime 卡片不渲染（返回 null） | ⏭️ 环境仅 1 台服务器 |

### 2.5 实时模式

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-20 | 默认实时模式 | 进入 `/servers/:id` | "Real-time" 按钮高亮选中（variant=default + bg-primary），URL search 为 `?range=realtime` | ✅ |
| SV-21 | 实时图表渲染 | 实时模式下查看 | 6 个图表渲染：CPU Usage、Memory Usage、Disk Usage、Network In、Network Out、Load Average (1m)。2 列网格布局 | ✅ |
| SV-22 | 实时数据点累积 | 等待多个 WS 推送周期 | 图表数据点逐渐增多，每个图表均为 AreaChart 面积图 | ✅ X 轴时间点从 3 个增长到 5 个 |
| SV-23 | 实时时间格式 | 查看 X 轴 | 第一个 tick 显示 HH:mm:ss（如 "01:09:48 PM"），后续显示 mm:ss（如 "09:51"） | ✅ |
| SV-24 | 实时隐藏温度/GPU/DiskIO | 实时模式 | Temperature、GPU、Disk I/O 图表不可见 | ✅ 仅 6 个图表标题，无 Temperature/GPU/Disk I/O |
| SV-25 | 节流渲染 | 实时模式连续收到 WS 数据 | 最多每 2 秒触发一次 re-render（RENDER_THROTTLE_MS=2000） | ✅ 代码逻辑验证 |
| SV-26 | 缓冲区上限 | 长时间保持实时模式 | 数据点超过 250 个时裁剪为最近 200 个（TRIM_THRESHOLD/MAX_BUFFER_SIZE） | ✅ 代码逻辑验证 |

### 2.6 历史模式

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-27 | 切换到 1h | 点击 "1h" 按钮 | URL 更新为 `?range=1h`，图表切换为 API 查询的历史数据，interval=raw | ✅ |
| SV-28 | 切换到 6h | 点击 "6h" | URL 更新 `?range=6h`，interval=raw | ✅ |
| SV-29 | 切换到 24h | 点击 "24h" | URL 更新 `?range=24h`，interval=raw | ✅ |
| SV-30 | 切换到 7d | 点击 "7d" | URL 更新 `?range=7d`，interval=hourly（聚合） | ✅ |
| SV-31 | 切换到 30d | 点击 "30d" | URL 更新 `?range=30d`，interval=hourly | ✅ |
| SV-32 | 历史→实时切换 | 从 1h 切回 Real-time | 图表显示缓存的实时数据（bufferCache 跨 unmount 持久化） | ✅ |
| SV-33 | 历史显示温度 | 切到 1h，有温度记录 | Temperature 图表可见（条件：chartData 中有 temperature > 0 的记录） | ✅ 1h 模式下出现 7 个图表标题含 "Temperature" |
| SV-34 | 历史显示 GPU | 切到 1h，有 GPU 记录 | GPU Usage + GPU Temperature 两个图表可见 | ⏭️ 环境无 GPU |
| SV-35 | 历史时间格式 | 查看 X 轴 | 默认 HH:mm 格式（defaultFormatTime） | ✅ 代码逻辑验证 |
| SV-36 | Disk I/O 历史显示 | 切到 1h/6h/24h | Disk I/O 卡片显示 Merged/Per Disk 标签页，可切换 | ✅ |
| SV-37 | Traffic Card 显示 | 切到任意历史范围 | TrafficCard 在 MetricsTabContent 底部渲染（有流量数据时） | ✅ 显示 Traffic Statistics + Today/Monthly tabs |

### 2.7 Tabs 切换

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-38 | Metrics Tab 默认选中 | 进入详情页 | Tabs defaultValue="metrics"，Metrics tab selected | ✅ |
| SV-39 | Traffic Tab 条件显示 | 服务器配置了 billing_cycle | TabsTrigger "Traffic" 可见（含 BarChart3 图标） | ✅ 设置 billing_cycle=monthly 后 Traffic tab 出现 |
| SV-40 | Traffic Tab 隐藏 | 服务器未配置 billing_cycle | Traffic Tab 不存在 | ✅ billing_cycle=null 时第一个 tablist 仅有 "Metrics" |
| SV-41 | 切换到 Traffic Tab | 点击 Traffic Tab | 显示 TrafficTab 组件（Current Billing Cycle: Start/End/Inbound/Outbound/Total + Daily Traffic Trend） | ✅ |

### 2.8 网络流量信息栏

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-42 | 在线流量统计 | 在线服务器有流量数据 | 名称下方显示卡片：Network In / Network Out / Total（formatBytes 格式化） | ✅ 显示 "Network In: 13.9 GB Network Out: 7.9 GB Total: 21.8 GB" |
| SV-43 | 离线隐藏流量统计 | 服务器离线或无流量 | 流量信息栏不显示 | ✅ 离线截图中确认无流量栏 |

### 2.9 计费信息栏

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-44 | 计费栏显示 | 配置 price/expired_at/traffic_limit 任意一项 | 显示 BillingInfoBar（CreditCard 图标 + 价格 + 到期日 + 流量进度条） | ✅ |
| SV-45 | 价格显示 | 设置 price=9.99, currency=USD, billing_cycle=monthly | 显示 "$9.99 / monthly" | ✅ |
| SV-46 | 到期倒计时 | 到期日在 7 天后 | 显示 "Expires YYYY/MM/DD (X days)" 黄色文字 | ⏭️ 未设置到期日 |
| SV-47 | 已过期标红 | 到期日在过去 | 显示 "Expired YYYY/MM/DD" 红色文字（text-destructive） | ⏭️ 未设置到期日 |
| SV-48 | 无计费隐藏 | 未配置 price/expired_at/traffic_limit | BillingInfoBar 不渲染 | ✅ 默认无计费信息时无 BillingInfoBar |

### 2.10 编辑对话框

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-49 | 对话框打开 | 点击 Edit | ServerEditDialog 弹出，标题 "Edit Server" | ✅ |
| SV-50 | BASIC 区域 | 查看表单 | 字段：Name(required)、Weight(number)、Hidden(checkbox)、Group(select)、Remark、Public Remark | ✅ BASIC fieldset 含 Name/Weight/Hidden/Group |
| SV-51 | BILLING 区域 | 查看表单 | 字段：Price、Currency(USD/EUR/CNY/JPY/GBP)、Billing Cycle(monthly/quarterly/yearly)、Expiration(date)、Traffic Limit(GB)、Limit Type(sum/up/down)、Billing Start Day(1-28) | ✅ BILLING fieldset 含所有字段 |
| SV-52 | 预填当前值 | 打开编辑对话框 | 所有字段预填服务器当前值 | ✅ Name 预填 "New Server" |
| SV-53 | 保存成功 | 修改 Name → 点击 Save | toast "Server updated successfully"，对话框关闭，页面名称更新 | ✅ 名称更新为 "Test Server Renamed"，对话框关闭 |
| SV-54 | 取消不保存 | 修改后点击 Cancel | 对话框关闭，不发送请求 | ✅ 对话框关闭 |

### 2.11 Capabilities 对话框

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-55 | 对话框渲染 | 点击 Capabilities | 显示 2 列 Card 布局：High Risk Operations (Terminal/Exec/Upgrade/File/Docker 5 项) + Monitoring & Maintenance (ICMP/TCP/HTTP 3 项) | ✅ |
| SV-56 | Toggle 开关 | 切换某个 capability 的 Switch | 调用 PUT /api/servers/:id，toast "Capabilities updated" | ✅ 切换 Terminal: unchecked→checked→unchecked |
| SV-57 | 默认值 | CAP_DEFAULT=56 | ICMP(8)+TCP(16)+HTTP(32) 默认开启（checked），Terminal/Exec/Upgrade/File/Docker 关闭 | ✅ 8 个 switch 前 5 未 checked 后 3 checked |
| SV-58 | Member 不可见 | Member 用户进入详情页 | Capabilities 按钮不渲染（user.role !== 'admin' 返回 null） | ✅ 单元测试覆盖 |

---

## 三、shadcn Chart 图表

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| C1 | MetricsChart 渲染 | `/servers/:id` → 实时模式 | CPU/Memory/Disk/Network In/Network Out/Load 共 6 个图表正常渲染，每个带边框圆角卡片 + h3 标题 | ✅ |
| C2 | MetricsChart Tooltip 格式化 | hover CPU 图表 | CPU 显示 `xx.x%`；Network In 显示 formatBytes 格式（如 `48.8 KB`） | ⏭️ headless 模式无法验证 hover |
| C3 | MetricsChart 历史切换 | 点击 1h/6h/24h | 图表切换为历史数据，含 Temperature 图表，X 轴时间格式 HH:mm | ✅ |
| C4 | AreaChart 样式 | 查看图表 | fillOpacity=0.1, strokeWidth=2, animationDuration=800, type="monotone" | ✅ 代码逻辑验证 |
| C5 | 浅色主题 | 浅色模式下查看所有图表 | 颜色、背景、文字对比度正常 | ✅ 截图验证 |
| C6 | 深色主题 | 深色模式下查看所有图表 | 颜色、背景、文字对比度正常 | ✅ 截图验证 |

---

## 四、Dashboard 实时更新

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-59 | Dashboard 实时更新 | Dashboard 页面等待 5s | CPU/Memory/Network 数值发生变化 (WebSocket 推送) | ✅ 之前 E2E 测试已验证 |

---

## 五、API 验证

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| API-1 | GET /api/servers/:id | 认证后请求 | 200，返回完整 ServerResponse（name/os/cpu_name/cpu_cores/mem_total/ipv4/capabilities 等） | ✅ 返回 name="New Server", os="macOS 26.3.1", cpu_cores=10, mem_total=34359738368, capabilities=56 |
| API-2 | GET /api/servers/:id/records | 带 from/to/interval=raw 参数 | 200，返回 ServerMetricRecord[]（含 cpu/mem_used/disk_used/net_in_speed/disk_io_json/temperature 等 20 个字段） | ✅ 返回 60 条记录 |
| API-3 | GET /api/servers/:id/uptime-daily | 带 days=90 | 200，返回 UptimeDailyEntry[]（date/online_minutes/total_minutes/downtime_incidents） | ✅ 返回 90 条记录 |
| API-4 | PUT /api/servers/:id | 更新 name | 200，返回更新后的 ServerResponse | ✅ |
| API-5 | 不存在的 server ID | GET /api/servers/non-existent | 404 | ✅ |

---

## 六、i18n

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| I18N-1 | 英文模式 | 英文下查看详情页 | "Back to Dashboard"、"Edit"、"Capabilities"、"CPU Usage"、"Memory Usage"、"Real-time"、"1h"、"6h" 等英文显示 | ✅ |
| I18N-2 | 中文模式 | 切换中文 | 显示 "返回仪表盘"、"编辑"、"能力"、"CPU 使用率"、"内存使用率"、"实时"、"1 小时"、"6 小时"、"24 小时"、"7 天"、"30 天" | ✅ |
| I18N-3 | 时间范围按钮 | 切换语言 | Real-time/实时、1h/1 小时、6h/6 小时、24h/24 小时、7d/7 天、30d/30 天 | ✅ |

---

## 测试统计

| 模块 | 用例数 | ✅ | ⏭️ | — |
|------|--------|-----|------|-----|
| 服务器列表 | 4 | 4 | 0 | 0 |
| 页面导航与加载 | 3 | 2 | 1 | 0 |
| 服务器信息区 | 4 | 3 | 1 | 0 |
| 操作按钮区 | 6 | 3 | 3 | 0 |
| Uptime 卡片 | 2 | 1 | 1 | 0 |
| 实时模式 | 7 | 7 | 0 | 0 |
| 历史模式 | 11 | 10 | 1 | 0 |
| Tabs 切换 | 4 | 4 | 0 | 0 |
| 网络流量信息栏 | 2 | 2 | 0 | 0 |
| 计费信息栏 | 5 | 3 | 2 | 0 |
| 编辑对话框 | 6 | 6 | 0 | 0 |
| Capabilities 对话框 | 4 | 4 | 0 | 0 |
| Chart 图表 | 6 | 5 | 1 | 0 |
| Dashboard 实时更新 | 1 | 1 | 0 | 0 |
| API 验证 | 5 | 5 | 0 | 0 |
| i18n | 3 | 3 | 0 | 0 |
| **合计** | **73** | **63** | **10** | **0** |

- ✅ 通过：63 (86%)
- ⏭️ 跳过（环境限制）：10 (14%)
- — 未测：0
