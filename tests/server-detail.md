# 服务器列表与详情页测试用例

## 前置条件

参照 [TESTING.md](../TESTING.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

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

### 2.1 基础渲染

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-5 | 系统信息 | 进入服务器详情页 | 显示 OS/CPU/RAM/Kernel 等系统信息 | — |
| SV-6 | Uptime 卡片 | 查看指标区域上方 | 显示 Uptime 百分比 + 90 天时间线 + 标签 + 图例 | — |

### 2.2 实时模式

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-7 | 实时模式默认 | 进入 `/servers/:id` | "Real-time" 按钮高亮选中（默认模式） | ✅ |
| SV-8 | 实时图表更新 | 实时模式下等待 10s | CPU/Memory/Disk/Network/Load 图表出现多个数据点，X 轴显示 mm:ss 格式 | ✅ |
| SV-9 | 实时首点时间格式 | 查看 X 轴第一个 tick | 显示 HH:mm:ss（含小时），后续 tick 显示 mm:ss | ✅ |
| SV-10 | 实时模式隐藏温度/GPU | 实时模式下查看 | Temperature 和 GPU 图表不可见（WS 数据不含温度和 GPU） | ✅ |

### 2.3 历史模式

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-11 | 实时→历史切换 | 点击 1h | 图表切换为历史数据，X 轴切换为 HH:mm 格式 | ✅ |
| SV-12 | 历史→实时切换 | 从 1h 切回 Real-time | 图表显示累积的实时数据（非空，之前积累的点保留） | ✅ |
| SV-13 | 历史模式显示温度 | 切换到 1h | 若有温度数据，Temperature 图表可见 | — |
| SV-14 | 离线服务器实时模式 | 离线服务器进入详情页 | 实时模式默认选中，图表为空（可接受） | — |
| SV-15 | 时间范围切换 | 点击 6h | 图表更新时间轴和数据 | ✅ |

### 2.4 Capability 开关

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-16 | Capabilities 展示 | 查看详情页底部 | 显示 6 个开关，默认 3 关 3 开 | ✅ |
| SV-17 | 全局 Capabilities | 访问 `/settings/capabilities` | 表格视图管理所有服务器的能力开关，支持搜索和批量选择 | ✅ |
| SV-18 | Capabilities 对话框 | admin 用户点击触发按钮 | 打开能力控制对话框 | — |

---

## 三、shadcn Chart 图表

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| C1 | MetricsChart 渲染 | `/servers/:id` → 实时模式 | CPU/Memory/Disk/Network/Load 图表正常渲染 | ✅ |
| C2 | MetricsChart Tooltip 格式化 | hover CPU 图表 | 显示 `xx.x%`；hover Network In → 显示 `xx.x MB/s` 格式 | ✅ |
| C3 | MetricsChart 历史切换 | 点击 1h/6h/24h | 图表切换为历史数据，X 轴时间格式正确 | — |
| C4 | LatencyChart 多线渲染 | `/network/:serverId` → 多个目标 | 图表显示多条彩色延迟线 | — |
| C5 | LatencyChart 目标隐藏/显示 | 点击 TargetCard 切换可见性 | 图表中对应线条正确隐藏/显示 | — |
| C6 | LatencyChart Tooltip 格式化 | hover 图表 | 显示 `xx.x ms` + 目标名称（非 target_id） | — |
| C7 | LatencyChart 颜色一致性 | 对比卡片与图表 | TargetCard 颜色圆点与图表线条颜色一一对应 | — |
| C8 | TrafficCard 展开态 | 服务器详情页 → 点击展开 | 日 BarChart + 小时 LineChart 正常渲染 | ✅ |
| C9 | TrafficCard BarChart Legend | 查看日 BarChart | 底部显示 ↓ In / ↑ Out 图例 | — |
| C10 | TrafficCard Tooltip 格式化 | hover 柱状图 | 显示格式化后的字节数（如 `12.5 MB`） | — |
| C11 | TrafficCard 颜色修复 | 查看 BarChart/LineChart | 柱状和线条颜色正确显示（修复前 hsl(oklch) 无效） | — |
| C12 | PingResultsChart 渲染 | `/settings/ping-tasks` → 展开任务 | 延迟图表正常渲染 | — |
| C13 | PingResultsChart 断线 | Ping 失败时间点 | 显示为断线而非连续 | — |
| C14 | PingResultsChart Tooltip | hover 图表 | 显示 `xx.xms` + 完整日期时间 | — |
| C15 | 浅色主题 | 浅色模式下查看所有图表 | 颜色、背景、文字对比度正常 | ✅ |
| C16 | 深色主题 | 深色模式下查看所有图表 | 颜色、背景、文字对比度正常 | ✅ |
| C17 | recharts outline | 查看图表元素 | 无多余 outline/focus ring | — |
| C18 | Tooltip 多系列指示器 | LatencyChart/TrafficCard tooltip | 行显示颜色指示器 + 系列标签（非纯文本） | — |

---

## 四、Dashboard 实时更新

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SV-19 | Dashboard 实时更新 | Dashboard 页面等待 5s | CPU/Memory/Network 数值发生变化 (WebSocket 推送) | ✅ |
