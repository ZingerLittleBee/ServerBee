# VPS 成本洞察设计

## 1. 背景

ServerBee 已经在 `servers` 表中保存了 VPS 的价格相关字段:

- `price`
- `currency`
- `billing_cycle`
- `expired_at`
- `traffic_limit`
- `traffic_limit_type`
- `billing_start_day`

这些字段目前主要用于编辑、详情页账单信息展示、流量周期计算和到期告警。它们还没有成为监控产品逻辑的一部分:服务器列表无法看出哪台机器最贵,详情页无法解释钱花在哪里,告警系统也无法基于成本和价值做判断。

本设计把 VPS 价格升级成一组可解释的成本洞察能力。第一版保持实用:在服务器列表、卡片和详情页中展示成本、燃烧速度和价值评分;不新增独立成本中心,不做汇率和预算系统。

产品气质采用"严肃功能打底,带一点克制的趣味表达"。ServerBee 仍然是监控工具,不是记账软件,更不是"恭喜你解锁烧钱成就"的玩具。

## 2. 目标与非目标

### 2.1 目标

- 让 `price` 参与真实产品逻辑,而不是只作为备注字段展示。
- 后端提供标准化成本洞察接口,避免前端各处重复和漂移计算。
- 在服务器列表和卡片中快速暴露成本与价值状态。
- 在服务器详情页展示单台 VPS 的成本拆解、当前周期燃烧进度和价值评分。
- 提供透明、可测试、非玄学的 `value_score`。
- 为后续成本告警、预算和独立成本页面预留稳定 API 与数据语义。

### 2.2 非目标

- 不做汇率转换。
- 不做预算管理。
- 不落库历史成本快照。
- 不新增独立成本页面。
- 不新增成本告警 UI。
- 不引入 AI 分析建议。
- 不修改 Agent 协议,成本计算完全由 Server 基于已有数据完成。
- 不把价格接入公开状态页,首版成本只面向登录后的管理界面。

## 3. 现状分析

| 维度 | 现状 |
|---|---|
| 价格字段 | `servers.price / currency / billing_cycle / expired_at` 已存在 |
| 计费周期 | `billing_cycle` 支持 `monthly / quarterly / yearly`,流量服务已有 `get_cycle_range()` |
| 流量额度 | `traffic_limit / traffic_limit_type / billing_start_day` 已用于流量 overview 和详情页 |
| 详情页 | `BillingInfoBar` 展示价格、到期时间、流量进度 |
| 列表页 | 表格和卡片暂无成本列或成本脚注 |
| 告警 | 已有到期告警和周期流量告警,暂无成本/价值告警 |
| API 风格 | Axum + `Json<ApiResponse<T>>`,DTO 使用 `utoipa::ToSchema` |
| 前端数据 | React Query hooks + OpenAPI types,列表通过 `/api/traffic/overview` 获取流量 overview |

当前最大问题不是缺字段,而是缺一个单一可信的成本计算服务。若直接在前端组件里分散计算,短期快,长期会导致列表、详情、告警之间语义不一致。

## 4. 方案选择

### 4.1 方案 A:轻量前端展示层

只在前端基于现有字段计算每秒消耗、今日消耗、本周期消耗和价值分。

- **优点**:实现快,不改数据库和 API。
- **缺点**:计算逻辑散落在组件中;后续告警、成本页和 API 消费者无法复用;测试边界弱。

### 4.2 方案 B:成本洞察 API(本设计选用)

新增后端 `CostService` 和只读 API,统一输出成本标准化结果、周期燃烧状态、资源成本和价值评分。前端只负责展示与交互。

- **优点**
  - 成本语义集中、可测试、可复用。
  - 未来接成本告警和成本页面不需要重算地基。
  - 遵守现有 Rust 服务层 + React Query 数据流。
- **缺点**
  - 第一版需要触碰 Rust 服务层、API、OpenAPI 和前端类型。

### 4.3 方案 C:成本系统雏形

一步加入成本页面、预算、历史成本表、汇率、成本告警和排行。

- **优点**:能力完整。
- **缺点**:首版范围过大,容易把 VPS 监控项目做成半个 FinOps 平台。

**结论:采用方案 B。** 第一版先让成本成为稳定的后端能力和核心页面洞察,避免范围膨胀。

## 5. 价格语义

### 5.1 `price` 跟随计费周期

`price` 的单位由 `billing_cycle` 决定:

| `billing_cycle` | `price` 语义 |
|---|---|
| `monthly` | 当前计费月价格 |
| `quarterly` | 当前季度计费周期价格 |
| `yearly` | 当前年度计费周期价格 |

例如:

- `price = 5, billing_cycle = monthly` 表示每月 5。
- `price = 15, billing_cycle = quarterly` 表示每季度 15。
- `price = 60, billing_cycle = yearly` 表示每年 60。

### 5.2 未配置周期不参与计算

只有 `price` 和合法 `billing_cycle` 都存在时,服务器才进入成本计算。

- 有价格、无周期:只展示原始价格,不计算每秒消耗、周期燃烧、价值分。
- 无价格:标记为未配置。
- 未知周期:标记为非法配置,不偷偷按 `monthly` 兜底。

### 5.3 货币不混算

第一版不做汇率转换。

- 单台服务器照常显示自己的 `currency`。
- `currency = NULL` 时按现有前端兼容行为视为 `USD`,并在响应中返回标准化后的 `currency = "USD"`。
- 全局 overview 按 `currency` 分组。
- 不输出跨币种总成本,避免把 USD、CNY、JPY 加成玄学数字。

## 6. 后端设计

### 6.1 新增 `CostService`

新增 `crates/server/src/service/cost.rs`。

职责:

- 校验成本配置是否完整。
- 标准化计费周期和周期日期。
- 计算每秒、每小时、每天、月等效成本。
- 计算当前周期已消耗金额和燃烧百分比。
- 计算资源单位成本。
- 聚合近 24 小时利用率数据。
- 结合资源、利用率、可靠性生成 `value_score`。
- 为 overview 输出列表友好的轻量数据。

不要把成本逻辑塞进 `TrafficService` 或 `ServerService`:

- `ServerService`:服务器配置 CRUD。
- `TrafficService`:流量周期和流量统计。
- `CostService`:价格和价值洞察。

### 6.2 复用数据源

`CostService` 读取:

- `server.price`
- `server.currency`
- `server.billing_cycle`
- `server.billing_start_day`
- `server.expired_at`
- `server.cpu_cores`
- `server.mem_total`
- `server.disk_total`
- `server.traffic_limit`
- `record` 最近 24 小时指标
- `uptime_daily` 最近 7 或 30 天在线率(若服务层已有可复用方法则复用)
- `AgentManager` 当前在线状态,作为历史不足时的兜底信号

### 6.3 周期计算

复用 `crate::service::traffic::get_cycle_range()`。

`cycle_start` 和 `cycle_end` 使用当前日期计算。`cycle_end` 保持现有语义:日期闭区间的结束日。成本计算时需要把天数视为包含首尾:

```text
cycle_days = (cycle_end - cycle_start).num_days() + 1
days_elapsed = clamp((today - cycle_start).num_days() + 1, 0, cycle_days)
days_remaining = max(cycle_days - days_elapsed, 0)
```

金额计算:

```text
cost_per_day = price / cycle_days
cost_per_hour = cost_per_day / 24
cost_per_second = cost_per_hour / 3600
cycle_cost_elapsed = cost_per_day * days_elapsed
cycle_cost_remaining = max(price - cycle_cost_elapsed, 0)
cycle_burn_percent = cycle_cost_elapsed / price * 100
```

`cost_per_month_equivalent` 用于同币种排序:

```text
monthly:   price
quarterly: price / 3
yearly:    price / 12
```

`cost_per_month_equivalent` 只表示账单等效,不用于当前周期燃烧。

### 6.4 输入校验

在 `ServerService::update_server()` 中补齐价格配置校验:

- `price` 必须大于等于 0。
- `billing_cycle` 只能是 `monthly / quarterly / yearly`。
- `currency` 允许为空;为空时成本接口标准化为 `USD`。非空值首版不强制限定到前端下拉选项,避免拒绝历史数据或 API 调用方使用其他 ISO 4217 货币代码。
- `billing_start_day` 已有 1..=28 DB trigger,服务层可提前返回更友好的 validation error。

价格为 0 合法,但不参与价值比较的性价比排序权重应特殊处理,避免除以 0。成本 insight 可返回 0 成本和 `excellent` 的资源价格并不合理,所以 `value_score` 应给出 `free_or_zero_price` reason,并跳过资源分同组分位计算。

### 6.5 API 设计

#### 6.5.1 `GET /api/cost/overview`

返回列表和卡片需要的轻量数据。该接口在 read router 中注册,所有已登录用户可读。

```rust
pub struct CostOverviewResponse {
    pub currencies: Vec<CurrencyCostSummary>,
    pub servers: Vec<ServerCostOverview>,
}

pub struct CurrencyCostSummary {
    pub currency: String,
    pub configured_server_count: u32,
    pub monthly_equivalent_total: f64,
    pub daily_total: f64,
    pub cycle_elapsed_total: f64,
}

pub struct ServerCostOverview {
    pub server_id: String,
    pub name: String,
    pub configured: bool,
    pub invalid_reason: Option<CostInvalidReason>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
    pub cost_per_second: Option<f64>,
    pub cost_per_hour: Option<f64>,
    pub cost_per_day: Option<f64>,
    pub cost_per_month_equivalent: Option<f64>,
    pub cycle_cost_elapsed: Option<f64>,
    pub cycle_burn_percent: Option<f64>,
    pub days_remaining: Option<i64>,
    pub value_score: Option<ValueScore>,
}
```

`/api/cost/overview` 不放进 `/api/traffic/overview`。流量页面不应该背成本逻辑的锅。

#### 6.5.2 `GET /api/servers/{id}/cost-insights`

返回单台服务器详情页使用的完整成本洞察。

```rust
pub struct ServerCostInsights {
    pub server_id: String,
    pub configured: bool,
    pub invalid_reason: Option<CostInvalidReason>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
    pub cycle_start: Option<String>,
    pub cycle_end: Option<String>,
    pub cycle_days: Option<i64>,
    pub days_elapsed: Option<i64>,
    pub days_remaining: Option<i64>,
    pub cost_per_second: Option<f64>,
    pub cost_per_hour: Option<f64>,
    pub cost_per_day: Option<f64>,
    pub cost_per_month_equivalent: Option<f64>,
    pub cycle_cost_elapsed: Option<f64>,
    pub cycle_cost_remaining: Option<f64>,
    pub cycle_burn_percent: Option<f64>,
    pub resource_value: Option<ResourceValue>,
    pub value_score: Option<ValueScore>,
}

pub struct ResourceValue {
    pub cost_per_cpu_core: Option<f64>,
    pub cost_per_gb_memory: Option<f64>,
    pub cost_per_gb_disk: Option<f64>,
    pub cost_per_tb_traffic_limit: Option<f64>,
}

pub struct ValueScore {
    pub score: f64,
    pub grade: ValueGrade,
    pub reasons: Vec<ValueReason>,
    pub confidence: ValueConfidence,
}
```

所有响应仍使用现有 `{ data: T }` 包裹。

### 6.6 枚举

建议使用 Rust enum + serde rename,避免字符串满天飞。

```rust
pub enum CostInvalidReason {
    MissingPrice,
    MissingBillingCycle,
    InvalidBillingCycle,
    InvalidPrice,
}

pub enum ValueGrade {
    Excellent,
    Good,
    Okay,
    Poor,
    Waste,
}

pub enum ValueReason {
    IdleBurn,
    SleepingMoney,
    GoodMemoryValue,
    GoodDiskValue,
    ExpensiveCpu,
    HealthyUptime,
    LowUptime,
    NoPriceCycle,
    InsufficientData,
    FreeOrZeroPrice,
}

pub enum ValueConfidence {
    High,
    Medium,
    Low,
}
```

前端通过 i18n 将 reason code 翻译成中英文文案。

## 7. `value_score` 设计

第一版评分必须透明,不要做聪明但不可解释的黑盒。

```text
value_score = resource_score 40 + utilization_score 35 + reliability_score 25
```

### 7.1 `resource_score`,40 分

衡量同一币种 fleet 内的资源性价比。

参与指标:

- `cost_per_cpu_core`
- `cost_per_gb_memory`
- `cost_per_gb_disk`
- `cost_per_tb_traffic_limit`

只和以下服务器比较:

- 同一 `currency`
- `price > 0`
- `billing_cycle` 合法
- 目标资源字段存在且大于 0

每个指标按分位数评分,单位成本越低分数越高。缺失资源字段时跳过该项,并用剩余项重新归一化。

如果一个服务器可参与的资源指标少于 2 个,`resource_score` 信心降级,并加入 `insufficient_data` reason。

### 7.2 `utilization_score`,35 分

衡量钱有没有被用起来。首版使用最近 24 小时 `record` 数据聚合:

- CPU 平均值
- 内存平均使用率
- 网络活动量或平均速度
- 磁盘 I/O 活动

建议规则:

- 极低利用率 + 高月等效成本:扣分,加入 `idle_burn`。
- 中等利用率且稳定:加分。
- 极高利用率不直接加满分,避免把过载误判为高价值。
- 数据不足时使用当前在线实时数据兜底,并降低 `confidence`。

这里不追求精确的容量规划。第一版目标是识别明显浪费,不是给机器颁发精算证书。

### 7.3 `reliability_score`,25 分

衡量花钱买到的在线稳定性:

- 优先使用最近 30 天 `uptime_daily` 计算在线率。
- 如果 30 天数据不足,使用最近 7 天。
- 如果仍不足,使用当前在线状态兜底,并降低 `confidence`。
- 离线且配置了价格和周期:加入 `sleeping_money`。
- 过期服务器不直接重扣价值分,但可以在详情页给续费提醒。到期更像运营风险,不是性价比本身。

### 7.4 grade 映射

```text
90-100 excellent
75-89  good
60-74  okay
40-59  poor
0-39   waste
```

输出 `score` 保留 1 位小数,前端展示可四舍五入为整数。

### 7.5 reason 文案原则

后端只返回 machine-readable reason code。前端本地化文案要短,列表不展示长句。

推荐文案方向:

- `idle_burn`:低利用率但持续花费。
- `sleeping_money`:服务器离线但账单仍在燃烧。
- `good_memory_value`:内存成本在同币种机器里较好。
- `expensive_cpu`:CPU 单价偏高。
- `healthy_uptime`:在线稳定性良好。
- `insufficient_data`:数据不足,评分信心较低。

趣味表达只出现在 tooltip 或详情页 reasons 中,不要污染表格主信息。

## 8. 前端设计

### 8.1 新增 hooks

```ts
export function useCostOverview()
export function useCostInsights(serverId: string)
```

列表和卡片使用 `useCostOverview()`。详情页使用 `useCostInsights(id)`。

不要在列表中逐台请求 `/api/servers/{id}/cost-insights`,避免 N+1。

### 8.2 服务器列表 table

新增 `Cost` 列,建议放在 `Network` 后面。

展示:

- 主值:`$4.20/mo eq.` 或 `¥0.97/day`
- 副值:`score 82 · good`
- 未配置:`not set`
- 有价格无周期:`price only`
- 非法配置:`invalid`

排序:

- 支持按 `cost_per_month_equivalent` 排序。
- 后续可支持按 `value_score.score` 排序。
- 不改变列表默认排序,避免用户打开列表突然变成烧钱排行榜。虽然这个诱惑很大,但别搞。

### 8.3 服务器卡片 grid

卡片已经很密,只加轻量脚注:

```text
¥0.03/h · good
```

浪费状态:

```text
¥0.03/h · waste
```

tooltip 展示:

- 每秒消耗
- 今日已消耗
- 本周期已消耗
- 主要评分原因

不在卡片上放长文本。

### 8.4 服务器详情页

把现有 `BillingInfoBar` 升级为 `CostInsightBar`。

摘要层:

- 当前价格:`$5.00 / monthly`
- 折算:`$0.16/day · $0.0068/h · $0.0000019/s`
- 本周期:`burned $2.71 · 54.2% · 14 days left`
- 价值:`82 good`

展开或下方轻量区块:

- `per CPU core`
- `per GB memory`
- `per GB disk`
- `per TB traffic limit`
- `reasons`

详情页可以出现更有个性的 reason 文案,但仍保持短句。

### 8.5 格式化

新增前端纯函数模块:

```text
apps/web/src/lib/cost.ts
```

职责:

- 金额格式化。
- 秒/小时/天/月等效格式化。
- grade 到样式的映射。
- reason code 到 fallback 文案 key 的映射。

金额格式使用 `Intl.NumberFormat`,失败时回退为 `${currency} ${amount.toFixed(2)}`。

## 9. 错误处理与缺失数据

成本接口应尽量返回部分可用结果,不要因为没填价格就 4xx。

| 场景 | 行为 |
|---|---|
| 无价格 | `configured = false`, `invalid_reason = missing_price` |
| 有价格无周期 | `configured = false`, `invalid_reason = missing_billing_cycle` |
| 未知周期 | `configured = false`, `invalid_reason = invalid_billing_cycle` |
| 价格小于 0 | 保存时拒绝;若历史数据存在,接口返回 `invalid_price` |
| 价格为 0 | 成本返回 0,评分跳过资源分位比较,reason `free_or_zero_price` |
| 多币种 | 单台正常展示,全局 summary 按币种分组 |
| 硬件数据缺失 | 资源单位成本字段为 `null`,评分归一化 |
| 历史记录不足 | 使用实时/当前状态兜底,`confidence = low` |

## 10. OpenAPI 与类型

所有后端 DTO 需要 `#[derive(Serialize, utoipa::ToSchema)]`。

新增路由后更新:

- `crates/server/src/openapi.rs`
- `apps/web/src/lib/api-types.ts`
- `apps/web/src/lib/api-schema.ts` 的 re-export 或临时手写类型

如果项目当前 API 类型生成流程不可用,第一版可以像 `TrafficResponse` 一样临时手写前端类型,但要在注释中标明后续由 OpenAPI 生成替代。

## 11. 测试策略

### 11.1 Rust 单元测试

`CostService`:

- `monthly / quarterly / yearly` 周期折算。
- 当前周期已消耗金额和燃烧百分比。
- `price + billing_cycle` 缺失组合。
- 未知 `billing_cycle`。
- `price = 0`。
- resource score 缺失字段归一化。
- grade 映射。
- 多币种分组 summary。

`ServerService`:

- price 非负校验。
- billing_cycle 枚举校验。

### 11.2 Rust API 集成测试

- `GET /api/cost/overview` 返回 configured 和 unconfigured 服务器。
- `GET /api/servers/{id}/cost-insights` 返回完整成本洞察。
- 未配置价格不返回 500。
- 多币种 summary 不混合。
- `currency = NULL` 的服务器归入 `USD` summary。
- 响应不包含 `token_hash` 或 `token_prefix`。

### 11.3 前端测试

`apps/web/src/lib/cost.test.ts`:

- 每秒/每小时/每天金额格式化。
- grade 样式映射。
- reason fallback key。

组件测试:

- `CostCell` configured。
- `CostCell` no price。
- `CostCell` price only。
- `CostFootnote` good / waste。
- 详情页有 insights 时展示 `CostInsightBar`。
- cost API 失败时页面不崩溃,保留基础 billing 信息或 fallback。

## 12. 第一版实施边界

首版只改:

- 后端成本服务和只读 API。
- 后端 server 更新校验。
- 前端 hooks。
- 服务器列表成本列。
- 服务器卡片成本脚注。
- 服务器详情页成本洞察条。
- i18n 文案。
- 对应测试。

首版不改:

- 数据库 schema。
- Agent 协议。
- public status page。
- alert rule schema。
- dashboard widget schema。
- 独立导航。

## 13. 后续扩展

本设计为以下功能预留空间,但首版不实现:

- 成本告警:
  - `value_score < N`
  - `reason contains sleeping_money`
  - `monthly_equivalent_total > budget`
- 独立成本页面:
  - 按币种总览。
  - 价值排行。
  - 浪费排行。
  - 续费日历。
- 预算:
  - 全局预算。
  - 分组预算。
  - 单机预算。
- 汇率:
  - 手动配置汇率。
  - 外部汇率源(需单独安全设计)。
- 历史成本快照:
  - 保存每日成本和评分,用于趋势和审计。

## 14. 验收标准

- 配置了价格和周期的 VPS 在列表中显示成本列,并可看到价值分。
- 卡片视图显示轻量成本脚注,不破坏现有密度。
- 详情页显示每秒、每小时、每天、本周期已消耗和资源单位成本。
- 没填价格或没填周期的服务器不会导致接口或页面报错。
- 多币种 overview 不混合汇总金额。
- `value_score` 的分数、grade 和 reasons 可由测试覆盖并解释。
- 后端成本计算逻辑只存在于 `CostService`,没有散落在前端组件里重复实现。
