# 月度流量统计测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

需为服务器配置 billing_cycle（通过编辑对话框或 API）以启用流量统计。

---

## 一、单元测试（Rust 后端）

### 1.1 TrafficService 单元测试（20 个用例）

运行：`cargo test -p serverbee-server service::traffic`

| # | 测试组 | 测试名 | 验证内容 |
|---|--------|--------|----------|
| UT1 | 增量计算 | `test_compute_delta_normal` | 正常增量：curr - prev |
| UT2 | 增量计算 | `test_compute_delta_both_restart` | 双方向同时重启（curr < prev）→ 使用 curr 原始值 |
| UT3 | 增量计算 | `test_compute_delta_single_direction_restart_in` | 仅入站重启，出站正常 |
| UT4 | 增量计算 | `test_compute_delta_single_direction_restart_out` | 仅出站重启，入站正常 |
| UT5 | 增量计算 | `test_compute_delta_zero` | 无变化 → delta 为 0 |
| UT6 | 计费周期 | `test_cycle_range_natural_month` | 自然月（anchor=1）→ 3/1~3/31 |
| UT7 | 计费周期 | `test_cycle_range_billing_day_15` | 自定义起始日 15 → 3/15~4/14 |
| UT8 | 计费周期 | `test_cycle_range_billing_day_before_anchor` | 当前日 < anchor → 回退到上一周期 |
| UT9 | 计费周期 | `test_cycle_range_quarterly` | 季付周期 → 4/1~6/30 |
| UT10 | 计费周期 | `test_cycle_range_yearly` | 年付周期 → 1/1~12/31 |
| UT11 | 计费周期 | `test_cycle_range_unknown_falls_back_to_monthly` | 未知周期类型 → 回退到月付 |
| UT12 | 预测算法 | `test_prediction_normal` | 正常预测：7 天数据 → 预计超限 |
| UT13 | 预测算法 | `test_prediction_too_early` | 不足 3 天 → 返回 None |
| UT14 | 预测算法 | `test_prediction_no_limit` | 无流量限额 → 返回 None |
| UT15 | DB 操作 | `test_upsert_traffic_hourly_accumulates` | 同一小时两次 upsert → bytes 累加 |
| UT16 | DB 操作 | `test_load_transfer_cache_from_traffic_state` | traffic_state 表 → HashMap 缓存加载 |
| UT17 | DB 操作 | `test_aggregate_daily_timezone_bucketing` | Asia/Shanghai 时区 → UTC 小时正确聚合到本地日期 |
| UT18 | 总览/历史 | `test_overview_empty` | 无服务器 → overview 返回空 vec |
| UT19 | 总览/历史 | `test_cycle_history_no_billing_cycle` | 服务器无 billing_cycle → overview 不含 + cycle_history 返回零流量 |
| UT20 | 序列化 | `test_server_traffic_overview_serialization` | ServerTrafficOverview JSON 字段完整性 |

> ✅ 21 个单元测试全部通过

### 1.2 集成测试

运行：`cargo test -p serverbee-server --test integration test_traffic`

| # | 测试函数 | 验证内容 |
|---|---------|---------|
| IT1 | `test_traffic_overview_api` | 空 overview 返回空数组 → 注册 Agent + 配置 billing_cycle → overview 包含服务器 → daily API 有效 |
| IT2 | `test_traffic_api_returns_data` | 注册 Agent → GET /api/servers/{id}/traffic → 验证 cycle_start/end/bytes/daily/hourly |
| IT3 | `test_server_billing_start_day` | 更新 billing_start_day=15 → GET 验证持久化 → 流量 API 反映 traffic_limit |

> ✅ 2 个集成测试全部通过（IT1+IT2 合并为 1 个测试函数）

### 1.3 前端单元测试

运行：`cd apps/web && bun run test -- --run use-traffic traffic-card`

| # | 测试文件 | 验证内容 |
|---|---------|---------|
| FT1 | `use-traffic.test.tsx` | useTraffic Hook 返回正确数据、空 serverId 不请求 |
| FT2 | `traffic-card.test.tsx` | TrafficCard 组件渲染、Tab 切换（Today/Monthly）、零流量时隐藏 |

> ✅ 前端测试全部通过（220 tests passing）

---

## 二、E2E 测试 — 流量总览页（/traffic）

### 2.1 页面加载与布局

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E1 | 页面加载 | 点击侧边栏 "Traffic" 导航 | 页面显示标题 "Traffic Overview"，加载骨架屏后显示内容 | ✅ |
| E2 | 统计卡片 | 查看页面顶部 4 张卡片 | 显示 Cycle Inbound / Cycle Outbound / Highest Usage / Servers > 80% | ✅ |
| E3 | 30 天趋势图 | 查看页面底部 AreaChart | X 轴日期(MM-DD)、两条曲线(Inbound 蓝 + Outbound 橙)、图例 + Tooltip | ✅ |

### 2.2 服务器排行表格

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E4 | 表格列展示 | 查看排行表格 | 显示 Server / Inbound / Outbound / Total / Limit / Usage / Days Left 列 | ✅ |
| E5 | 默认排序 | 进入页面 | 表格按 Total 降序排列（列头带 ↓ 箭头） | ✅ |
| E6 | 点击排序 | 点击 "Server" 列头 | 按服务器名排序，再次点击切换升降序 | ✅ |
| E7 | Usage 进度条颜色 | 查看 Usage 列 | 0-70% 绿色、70-90% 黄色、90%+ 红色 | ✅ |
| E8 | 无限额显示 | 查看未配置 traffic_limit 的服务器 | Limit 列显示 "Unlimited" badge | ✅ |
| E9 | 无 billing_cycle 时 | 所有服务器均无 billing_cycle 配置 | 表格显示 "No servers with traffic data yet." | ✅ |

---

## 三、E2E 测试 — 服务器详情页 Traffic Tab（/servers/:id）

### 3.1 Tab 显示条件

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E10 | 有 billing_cycle 时 | 进入已配置 billing_cycle 的服务器详情 | Tab 栏显示 "Traffic" 选项卡 | ✅ |
| E11 | 无 billing_cycle 时 | 进入未配置 billing_cycle 的服务器详情 | Tab 栏无 "Traffic" 选项卡 | ✅ |

### 3.2 Cycle Overview 卡片

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E12 | 周期信息 | 点击 Traffic Tab | 显示 Current Billing Cycle 卡片，包含 Start / End 日期 | ✅ |
| E13 | 流量统计 | 查看卡片底部 | 显示 Inbound / Outbound / Total 字节数 | ✅ |
| E14 | 进度条 | 已配置 traffic_limit | 显示进度条：[已用 / 限额] + 百分比，颜色按阈值 | ✅ |

### 3.3 Daily Trend 图表

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E15 | 日柱状图 | 点击 Traffic Tab | 显示 Daily Traffic Trend 卡片，无数据时显示 "No daily traffic data available." | ✅ |
| E16 | 时间范围切换 | 点击 [7d] / [30d] / [90d] 按钮 | 图表数据范围相应变化，活跃按钮高亮 | ✅ |
| E17 | Tooltip | 鼠标悬停柱子 | 显示日期 + In/Out 字节数 | — |

### 3.4 历史周期对比

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E18 | 历史对比图 | 查看 Historical Cycle Comparison | 水平柱图显示最多 6 个历史周期 | ✅ |
| E19 | 仅当有历史时显示 | 新服务器（无历史数据） | 历史对比区域不显示 | ✅ |

### 3.5 TrafficCard（Metrics Tab 中）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E20 | 流量卡片显示 | 查看 Metrics Tab | 显示 Traffic Statistics 卡片，Today/Monthly Tab | ✅ |
| E21 | Today Tab | 点击 Today | 显示今日小时柱状图 (HH:00 格式)，堆叠柱 In(蓝) + Out(绿) | ✅ |
| E22 | Monthly Tab | 点击 Monthly | 显示本月日柱状图 (MM-DD 格式)，堆叠柱 In(蓝) + Out(绿) | ✅ |
| E23 | 底部统计 | 查看 CardFooter | 周期日期范围 "2026-03-01 ~ 2026-03-31" + ↓ In / ↑ Out / Total | ✅ |
| E24 | 无流量时隐藏 | 新注册服务器无流量 | TrafficCard 不显示 | ✅ |

### 3.6 BillingInfoBar 进度条

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E25 | 进度条显示 | 配置 traffic_limit → 进入详情页 | 顶部显示进度条 "6.0 GB / 1.0 TB" + "0.6%" | ✅ |
| E26 | 颜色阈值 | 查看进度条颜色 | 0-70% 绿色 → 70-90% 黄色 → 90%+ 红色 | ✅ |
| E27 | 预测虚线 | 已过 3 天 + 有 traffic_limit | 进度条显示虚线预测标记 | — |
| E28 | 超限警告 | 预测超限 | 红色文字 "Predicted to exceed limit (~XX%)" | — |
| E29 | 限额类型 | 设置 traffic_limit_type=up | 进度条仅计算出站(bytes_out) | — |

---

## 四、API 端点直接测试

用 curl 或 Swagger UI (`/swagger-ui/`) 验证：

### 4.1 流量查询端点

| # | 端点 | 验证内容 |
|---|------|---------|
| A1 | `GET /api/servers/{id}/traffic` | 返回 cycle_start/end、bytes_in/out/total、daily[]、hourly[]、prediction、usage_percent |
| A2 | `GET /api/traffic/overview` | 返回 Vec<ServerTrafficOverview>，仅含配置了 billing_cycle 的服务器 |
| A3 | `GET /api/traffic/overview/daily?days=30` | 返回 30 天全局汇总 DailyTraffic 数组 |
| A4 | `GET /api/traffic/{server_id}/cycle?history=6` | 返回 current + 最多 6 个 history 周期 |

> ✅ A1-A3 测试通过

### 4.2 计费配置端点

| # | 端点 | 验证内容 |
|---|------|---------|
| A5 | `PUT /api/servers/{id}` (billing_cycle=monthly) | 设置后 traffic API 返回正确周期范围 |
| A6 | `PUT /api/servers/{id}` (billing_start_day=15) | 周期变为 15~14 |
| A7 | `PUT /api/servers/{id}` (billing_start_day=0) | 返回错误（trigger 拦截，必须 1-28） |
| A8 | `PUT /api/servers/{id}` (traffic_limit=1099511627776) | 设置 1TB 限额，API 返回 usage_percent |

### 4.3 权限测试

| # | 端点 | 验证内容 |
|---|------|---------|
| A9 | `GET /api/servers/{id}/traffic` (未登录) | 返回 401 |
| A10 | `GET /api/traffic/overview` (Member 用户) | 返回 200（读端点所有认证用户可访问） |

> ✅ A9 测试通过（401 confirmed）

---

## 五、后台任务与数据处理

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| S1 | 流量数据写入 | Agent 连接并上报 → 等待 60s | traffic_hourly 表有记录 | ✅ |
| S2 | 增量正确性 | Agent 上报 net_in_transfer=1000 → 60s 后上报 1500 | hourly delta=500 | ✅ |
| S3 | Agent 重启恢复 | 停止 Agent → 重启 → cumulative 从 0 开始 | delta 使用原始值（不产生负数） | ✅ |
| S4 | 日聚合 | 等待 aggregator 运行（每小时） | traffic_daily 有记录，bytes 与 hourly SUM 一致 | ✅ |
| S5 | 时区聚合 | 配置 timezone=Asia/Shanghai | UTC 20:00 归入次日（本地 04:00） | ✅ |
| S6 | 数据清理 | 配置 traffic_hourly_days=1 → 等待 cleanup | 超过 1 天的 hourly 记录被删除 | — |

---

## 六、i18n 国际化

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| I1 | 中文界面 | 切换中文 → 访问 /traffic | 侧边栏「流量统计」、面包屑「流量统计」 | ✅ |
| I2 | 英文界面 | 切换英文 → 访问 /traffic | 标题 "Traffic Overview"、表头英文 | ✅ |
| I3 | Traffic Tab 中文 | 中文 → 查看服务器 Traffic Tab | "当前计费周期"、"每日趋势"、"历史对比" 中文 | ✅ |

---

## 七、响应式

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| R1 | 移动端总览页 | 窗口宽度 < 768px 查看 /traffic | 统计卡片 2x2 网格或单列，表格可横向滚动 | ✅ |
| R2 | 移动端 Traffic Tab | 窗口宽度 < 768px 查看 Traffic Tab | 图表自适应宽度，按钮组可点击 | ✅ |
