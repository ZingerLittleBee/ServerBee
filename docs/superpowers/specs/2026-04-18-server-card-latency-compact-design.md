# Server Card 延迟/丢包紧凑展示设计

## 目标

重做 `ServerCard` 底部的延迟和丢包展示区，达到"一眼看清"的效果：突出当前状态数字、合并延迟和丢包的严重度信号、去掉重复的视觉层。

## 背景

当前 `apps/web/src/components/server/server-card.tsx` 的延迟区由三部分组成：

- **头部摘要行**：`延迟` 标签 + 当前平均延迟（延迟色）+ `·` + 平均丢包（丢包色）
- **柱状图**（`h-7`）：每根柱按**延迟**着色
- **丢包细条**（`h-1`）：每格按**丢包率**着色

存在的问题：

1. **两条图表重复承载时间轴**。柱状图和细条颜色维度不同，但数据点一一对应，肉眼需要同时扫描两行才能理解"这一刻同时发生了什么"。
2. **4px 的丢包细条太薄**。在 3 列 1920px 布局下实际高度 ~4px，色带辨识度差，远看几乎无效。
3. **当前数字不够醒目**。文字大小 12px，被柱状图颜色抢走注意力，"现在状况如何"不是第一眼信息。
4. **失败点（100% 丢包）和单纯高延迟视觉相同**。两者都只是"更高的红柱"，用户要去看细条才能确认是丢包还是延迟尖峰。

## 设计

### 1. 整体结构（三区降为两区）

```
┌───────────────────────────────────────────────────┐
│ 42ms              ● 丢包 2.3%                     │  ← 头部（紧凑型）
│ ▃▁▂▂▃█▂▁▂▂▂▂▁▂▃▂▂▇▂▂▃▂▂▂▂▃▂▂                      │  ← 严重度柱状图（h-8）
└───────────────────────────────────────────────────┘
```

相比当前去掉 `h-1` 丢包细条，信号合并到柱颜色里。

### 2. 头部（紧凑型）

```tsx
<div className="flex items-baseline justify-between">
  <span className={latencyColorClass(currentAvgLatency, { failed })}>
    <span className="font-semibold text-lg tabular-nums">{formatLatency(currentAvgLatency)}</span>
    <span className="ml-0.5 text-[10px] text-muted-foreground font-medium">ms</span>
  </span>
  <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
    <span className={`size-1.5 rounded-full ${lossDotColor}`} />
    <span>丢包 <strong className={lossColorClass}>{formatPacketLoss(loss)}</strong></span>
  </div>
</div>
```

关键点：

- **延迟主字** `18px` 粗体，`tabular-nums` 防抖动；`ms` 单位小 10px 低对比度跟随
- **丢包副文** `11px`，**圆点按丢包单独取色**（独立于延迟），因此"延迟健康但丢包琥珀"的场景也能在圆点看到告警
- 失败态（平均丢包 == 100%）：延迟字显示为 `—`（单位 `ms` 保留但变弱），丢包数字显示 `100%` 红色

### 3. 严重度阈值

沿用现有 `apps/web/src/lib/network-latency-constants.ts` 的阈值（已在代码库存在，不引入新常量）：

| 状态 | 条件 | 颜色 | 用法 |
|------|------|------|------|
| `healthy` | 延迟 < 300ms 且 丢包 < 1% | `#10b981` (emerald) | 柱、文字 |
| `warning` | 延迟 ≥ 300ms 或 1% ≤ 丢包 < 5% | `#f59e0b` (amber) | 柱、文字 |
| `severe` | 丢包 ≥ 5%（未失败） | `#ef4444` (red) | 柱、文字 |
| `failed` | 丢包 == 100%（`NETWORK_FAILURE_PACKET_LOSS_RATIO`） | `#ef4444` + 斜线纹理 | 柱 |

严重度计算遵循"取最差"原则：延迟和丢包分别映射到 `healthy/warning/severe/failed`，合并时取更严重的那个。

### 4. 柱颜色函数

新增 `getCombinedSeverity` 和 `getCombinedBarColor`（放在 `network-latency-constants.ts`）：

```ts
export function getCombinedSeverity({
  latencyMs,
  lossRatio
}: {
  latencyMs: number | null | undefined
  lossRatio: number | null | undefined
}): LatencyStatus | 'severe' {
  if (lossRatio != null && lossRatio >= NETWORK_FAILURE_PACKET_LOSS_RATIO) return 'failed'
  if (lossRatio != null && lossRatio >= 0.05) return 'severe'
  if (latencyMs == null && lossRatio == null) return 'unknown'
  const latencyWarn = latencyMs != null && latencyMs >= LATENCY_HEALTHY_THRESHOLD_MS
  const lossWarn = lossRatio != null && lossRatio >= 0.01
  if (latencyWarn || lossWarn) return 'warning'
  return 'healthy'
}
```

类型扩展：`LatencyStatus` 增加 `'severe'` 值（或者直接和 `failed` 合为红色族，实现上看哪种更简洁）。

### 5. 失败柱渲染（满格红柱 + 斜线）

失败点（100% 丢包）**没有可测延迟**，需要两件事：

1. **满格高度**：失败点 value 固定为当前 Y 轴 domain 的最大值，或使用自定义 `shape` 直接渲染全高矩形
2. **斜线纹理**：在 Recharts `<defs>` 里定义一次，失败 Cell 引用

```tsx
<BarChart>
  <defs>
    <pattern id="latency-fail-stripe" width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
      <rect width="5" height="5" fill="#ef4444" />
      <line x1="0" y1="0" x2="0" y2="5" stroke="rgba(0,0,0,0.25)" strokeWidth="2" />
    </pattern>
  </defs>
  <Bar dataKey="value" shape={<SeverityBar />}>
    {latencyPoints.map(point => ...)}
  </Bar>
</BarChart>
```

`SeverityBar` 自定义 shape 的职责：

- 对非失败点：用常规矩形 + 严重度色填充，`radius=2`，高度按 `latencyMs` 在 Y 轴上的比例
- 对失败点：矩形高度强制为 100%（Y 轴 domain 顶），`fill="url(#latency-fail-stripe)"`

**为什么用 `shape` 而不是 `Cell` fill**：要让失败点高度覆盖整个可用空间（而数据 value 是 0 或 null），`Cell fill` 改不了高度。`shape` 可以在渲染时用 `height = ySpan` 覆写。

Y 轴 domain：默认由 Recharts 自动根据数据推断。为了保证"满格"失败柱真正撑满，显式设 `<YAxis hide domain={[0, 'dataMax']} />`（或传入显式最大值）。如果一整段都是失败，`dataMax` 退化为 0 —— 在 `SeverityBar` 里兜底用固定 max 比如 500ms。

### 6. 头部颜色函数（圆点 + 副文）

丢包圆点和丢包文字**不走合并严重度**，单独按丢包率取色（沿用当前 `getLossTextClassName` 的分档）：

| 丢包率 | 颜色 |
|--------|------|
| `null` / 无数据 | `text-muted-foreground` / `var(--color-muted)` |
| `< 1%` | `#10b981` (emerald) |
| `< 5%` | `#f59e0b` (amber) |
| `≥ 5%` | `#ef4444` (red) |

用例：

- 延迟 30ms + 丢包 3% → 延迟字绿、丢包字/圆点琥珀（用户第一眼看到绿色，但右上圆点告诉他丢包异常）
- 延迟 400ms + 丢包 0% → 延迟字琥珀、丢包字/圆点绿（延迟问题，丢包没事）

实现：保留现有 `getLossTextClassName`（tooltip 仍在用）。圆点背景色复用同一阈值：新增 `getLossDotBgClass`（或内联三元，看最终 JSX 简洁度决定）。

### 7. 保留的能力

- **多目标 tooltip**：`NetworkChartTooltip` 继续按原样渲染每个目标的延迟 + 丢包，不改
- **无数据时隐藏区块**：现有 `latencyPoints.length > 0` 条件保持
- **动画关闭**：`isAnimationActive={false}` 保持
- **数据管线**：`useNetworkOverview`、`useNetworkRealtime`、`buildServerCardNetworkState` 不改
- **详情页 (`servers/$id`) 的同类图表**：本次不动（详情页屏幕空间大，分区展示更合适）

### 8. 删除的代码

| 位置 | 删除内容 | 原因 |
|------|----------|------|
| `server-card.tsx` L394-410 | `<ChartContainer>` h-1 loss strip | 信号合并到主柱 |
| `server-card.tsx` L78-89 | `getLossStripColor` | 仅 strip 在用 |
| `server-card.tsx` L29-31 | `LOSS_STRIP_CONFIG` | 配套删除 |

### 9. 新增的代码

| 位置 | 新增内容 |
|------|----------|
| `network-latency-constants.ts` | `getCombinedSeverity`、`getCombinedBarColor`、`getLossDotColor`（或放 server-card 内部） |
| `server-card.tsx` | `SeverityBar` 自定义 shape 组件、`<defs>` 里的 `latency-fail-stripe` pattern |
| `server-card.tsx` | 新的紧凑头部 DOM（替换 L356-370） |

## 非目标

- **不改协议**：agent 和 server 的数据格式不动
- **不改详情页**：详情页的 `latency-chart.tsx` 本次不改
- **不引入新的严重度阈值**：完全复用 `network-latency-constants.ts` 现有常量
- **不改 tooltip 内容**：现有富 tooltip 按目标展开的设计继续用
- **不改 i18n key**：头部文字用新的 i18n key（`card_latency_hero`、`card_packet_loss_sub`），但现有的 `card_latency` 等 key 如果没人用就删

## 测试

这是纯 UI 呈现改动，按 `CLAUDE.md` 的约定："For tiny CSS-only or presentational class changes with no logic or behavior changes, default to minimal verification instead of running tests."

验证步骤：

1. `bun run typecheck` 通过
2. `bun x ultracite check` 通过
3. `make web-dev-prod` 启动 prod-proxy，目视验证：
   - 健康服务器：绿色柱 + 绿延迟 + 绿圆点
   - 有丢包但延迟正常的服务器：绿延迟字 + 琥珀/红圆点，对应柱体琥珀
   - 有失败记录的服务器：柱状图里有满格红斜线柱，tooltip hover 上去仍显示每个目标详情
4. 切换深色模式，确认颜色对比度 OK
5. 颜色对比测试（模拟色盲）：失败柱的斜线纹理即使在红绿色盲下也能辨识出"异常"

## 风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| Recharts 自定义 `shape` 实现细节出错（失败柱不满格） | 失败信号不明显 | 在 `SeverityBar` 里显式计算 `yScale.range()` 的顶端 |
| Y 轴 domain 退化为 `[0, 0]`（全是失败点）| 柱体渲染异常 | 在 shape 里兜底 min domain 500ms |
| 丢包圆点颜色 vs 延迟字颜色不一致，用户困惑 | 认知负担 | 在 tooltip 里清晰展示两个独立值；设计文档明确"圆点 = 丢包独立信号"的意图 |
