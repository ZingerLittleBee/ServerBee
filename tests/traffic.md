# 月度流量统计测试用例

## 前置条件

参照 [TESTING.md](../TESTING.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

需为服务器配置 billing_cycle（通过编辑对话框或 API）以启用流量统计。

---

## 一、自动化测试覆盖

### 单元测试

通过 `cargo test -p serverbee-server service::traffic` 运行：

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

### 集成测试

位于 `crates/server/tests/integration.rs`：

| 测试名 | 流程 |
|--------|------|
| `test_traffic_overview_api` | 空 overview 返回空数组 → 注册 Agent + 配置 billing_cycle → overview 包含服务器 + 验证字段 → daily API 有效 |
| `test_traffic_api_returns_data` | 注册 Agent → `GET /api/servers/{id}/traffic` → 验证响应包含 cycle_start/cycle_end/bytes_*/daily/hourly |
| `test_server_billing_start_day` | 更新 billing_start_day=15 → GET server 验证持久化 → 流量 API 反映 traffic_limit |

---

## 二、E2E 手动验证

### 2.1 数据写入与计算

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| T1 | 流量数据写入 | Agent 连接并上报 → 等待 60s | `traffic_hourly` 表有记录 | — |
| T2 | 增量正确性 | Agent 上报 net_in_transfer=1000 → 60s 后上报 1500 | hourly delta=500 | — |
| T3 | Agent 重启恢复 | 停止 Agent → 重启 → 上报的 cumulative 从 0 开始 | delta 使用原始值（不产生负数） | — |
| T4 | 日聚合 | 等待 aggregator 运行（每小时） | `traffic_daily` 表有记录，bytes 与 hourly SUM 一致 | — |
| T5 | 时区聚合 | 配置 `SERVERBEE_SCHEDULER__TIMEZONE=Asia/Shanghai` | UTC 20:00 的 hourly 记录归入次日（本地 04:00） | — |
| T6 | 流量 API 响应 | `GET /api/servers/{id}/traffic` | 返回 cycle_start/end、bytes_in/out/total、daily[]、hourly[] | — |

### 2.2 服务器详情页 Traffic Tab

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| T7 | BillingInfoBar 进度条 | 设置 traffic_limit → 服务器详情页 | 显示进度条（used/limit + 百分比） | — |
| T8 | 进度条颜色阈值 | 查看进度条 | 0-70% 绿色 → 70-90% 黄色 → 90%+ 红色 | — |
| T9 | 预测标记 | 已过 3 天 + 设有 traffic_limit | 进度条显示虚线预测标记 | — |
| T10 | 超限警告 | 预测超限 | 进度条下方显示红色警告文字 | — |
| T11 | TrafficCard 折叠展开 | 点击流量统计卡片 | 展开显示日柱状图 + 小时折线图，再次点击折叠 | — |
| T12 | 日柱状图 | 展开 TrafficCard | 柱状图显示每日 in/out 堆叠，hover 显示 tooltip | — |
| T13 | 小时折线图 | 展开 TrafficCard | 折线图显示今日小时 in/out，X 轴 HH:00 格式 | — |
| T14 | 无流量时隐藏 | 新注册服务器（无流量数据） | TrafficCard 不显示 | — |
| T25 | 服务器 Traffic Tab | 设置 billing_cycle → /servers/:id | Traffic Tab 可见，周期进度条 + 日柱图 + 历史对比 | — |

### 2.3 计费配置

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| T15 | 编辑 billing_start_day | 编辑对话框 → 设置计费起始日为 15 → 保存 | 流量 API 周期变为 15~14 | — |
| T16 | billing_start_day 范围 | 输入 0 或 29 → 保存 | 保存失败（trigger 拦截） | — |
| T17 | 自然月回退 | 清空 billing_start_day → 保存 | 周期回到 1~月末 | — |
| T18 | 流量限额类型 | 设置 traffic_limit_type=up | 进度条仅显示上传用量 | — |

### 2.4 流量总览页（/traffic）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| T23 | 流量总览排行 | 访问 /traffic | 表格按用量排序，最高用量服务器排第一 | — |
| T24 | 流量总览趋势图 | 访问 /traffic | 30 天趋势 AreaChart 显示入站/出站两条线 | — |

### 2.5 其他

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| T19 | 数据保留清理 | 配置 `traffic_hourly_days=1` → 等待 cleanup | 超过 1 天的 hourly 记录被删除 | — |
| T20 | 告警集成 | 创建 transfer_all_cycle 告警规则 → cycle_interval=billing | 流量超限时触发告警 | — |
| T21 | i18n 中文 | 切换中文 | "流量统计"、"每日流量"、"今日小时流量"、"预计将超出限额" 正确显示 | — |
| T22 | i18n 英文 | 切换英文 | "Traffic Statistics"、"Daily Traffic"、"Today's Hourly Traffic" 正确显示 | — |
