# ServerCard 双列布局重设计

**Date**: 2026-05-15
**Status**: Approved
**Scope**: `apps/web/src/components/server/server-card.tsx` 及其外层 `apps/web/src/components/dashboard/widgets/server-cards.tsx`

## 背景

当前 ServerCard 在宽屏 dashboard 下显示效果偏宽,主指标(CPU/内存/磁盘/流量)排成 4 列 Ring,占用横向空间过多;延迟趋势用单根连续 BarChart(SeverityBar)合并表达延迟+丢包综合严重度,无法分别看清两个维度。

用户希望:

1. 把卡片主体改成"左右两列"的紧凑结构(2×2 主指标网格)
2. 把网络质量行从单根合并柱状图改成 **左右并排的两个方块栅格**:左侧延迟、右侧丢包
3. 卡片宽度受约束 (`min 320px / max 480px`),不再被 widget config 列数硬控
4. **保留**现状所有信息(磁盘 R/W、load、processes、TCP/UDP、swap、流量剩余、cost footnote)
5. 新增渲染 `server.tags` 作为底部 ISP chip(用户在 server 配置里手工设置的标签)

不变更:
- WebSocket 协议 / `ServerMetrics` 字段集
- `buildServerCardNetworkState` 公开签名(仅放宽 `MAX_TREND_POINTS` 常量)
- 国际化 key 与现有 RingChart / CompactMetric 组件

## 布局结构

```
┌───────────────────────────────────────────────┐ 卡片宽度 320 ~ 480px
│ 🇭🇰 Gomami-Turin-Air 🐧       [up] 🟢          │ Header
├───────────────────────────────────────────────┤
│  ⬤ CPU      0.00%   │   ⬤ 内存    4.28%       │ 2×2 RingChart
│    load 0.12        │   1.2 / 28 GB           │   (ring + 标签 / 副文案)
│  ──────────────────  ──────────────────────   │
│  ⬤ 磁盘     3.3%    │   ⬤ 月度    8.5%        │
│    52 / 1.5 TB      │   174GB/2TB · 剩 23天    │
├───────────────────────────────────────────────┤
│  ↓ 56.45 Kbps          ↑ 18.88 Kbps           │ 速度行 (2 列 CompactMetric)
│  R 12.3 KB · W 8.1 KB    L5 0.8 · L15 0.5     │ 磁盘 RW + load5/15 (2 列)
├───────────────────────────────────────────────┤
│ 🕓 延迟 8 ms        │  📡 丢包  1.0%           │ 网络质量行: 左右两个独立方块栅格
│ ██▒██████████████   │  ███▒█▒████████████      │
├───────────────────────────────────────────────┤
│ ⏱ 60天 · swap 12% · proc 132 · tcp 45 · udp 7 │ Misc inline
│ 💰 $5/月 · 流量剩 23 天                         │ CostFootnote (现状逻辑)
├───────────────────────────────────────────────┤
│ [电信CN2 GIA] [联通AS9929] [移动CMI]           │ tags chips (新)
└───────────────────────────────────────────────┘
```

## 详细设计

### 1. 外层容器 (`ServerCardsWidget`)

把固定列数改为响应式列数:

```tsx
// before:
gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`

// after:
gridTemplateColumns: `repeat(auto-fill, minmax(320px, 1fr))`
```

行为:
- `config.columns` 字段不再用于 grid template;保留为兼容字段但忽略
- 浏览器自动按容器宽度排列卡片,每张最少 320px、最多受 `max-width: 480px` 限制
- 卡片不足以填满一行时仍按 `1fr` 拉伸,但不超过 `max-width`

注: 这是一次行为变化。如果用户之前手动设置过 `config.columns`,体感上是 "卡片自己挑列数,不再受配置影响"。后续 widget config UI 可以隐藏 columns 字段(本次不做)。

### 2. ServerCard 容器

```tsx
<div className="flex w-full min-w-[320px] max-w-[480px] flex-col gap-2 rounded-lg border bg-card p-3 shadow-sm">
```

- `gap-2` (8px) 在区块之间统一间距
- `p-3` 比现在的 `p-4` 略紧
- 内部不再用 `mb-3` 之类的散落 margin,统一靠 `gap`

### 3. Header 区

保持现状:国旗 + OS emoji + 名称 + UpgradeJobBadge + StatusBadge。
仅微调:`gap-1.5` → `gap-1`,字号 `text-sm` → `text-[13px]`。

### 4. 2×2 主指标区

替换现有 4 列 ring + 文本网格,改成 `grid grid-cols-2 gap-x-3 gap-y-2`:

```tsx
<div className="grid grid-cols-2 gap-x-3 gap-y-2">
  <RingMetric label={t('col_cpu')} value={cpu} subText={`load ${loadLabel}`} color={...} />
  <RingMetric label={t('col_memory')} value={memPct} subText={`${used} / ${total}`} color={...} />
  <RingMetric label={t('col_disk')} value={diskPct} subText={`${used} / ${total}`} color={...} />
  <RingMetric label={t('card_traffic_quota')} value={trafficPct} subText={`${used} / ${total}`} color={...} />
</div>
```

新增内部组件 `RingMetric`(同文件内,不导出):

```tsx
function RingMetric({ label, value, subText, color }) {
  return (
    <div className="flex items-center gap-2">
      <RingChart color={color} value={value} compact />   // 见下方说明
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center justify-between gap-1">
          <span className="truncate text-[11px] text-muted-foreground">{label}</span>
          <span className="font-medium text-xs tabular-nums">{value.toFixed(1)}%</span>
        </div>
        <span className="truncate text-[10px] text-muted-foreground tabular-nums">{subText}</span>
      </div>
    </div>
  )
}
```

RingChart 新增 `compact?: boolean` prop,在 compact 模式下尺寸从默认改为 28px,且 label 不在 ring 内部渲染(因为外部已有 label 文案)。如果给 `compact` 加 prop 影响面太大,也可以直接在 ServerCard 里把 RingChart 包一个固定尺寸 div,让 RingChart 内部 label 隐藏。

### 5. 速度与磁盘 R/W + load 行

```tsx
<div className="grid grid-cols-2 gap-x-3 gap-y-1 rounded-md bg-muted/40 px-2 py-1.5">
  <CompactMetric label={<ArrowDown />} value={renderSpeed(net_in_speed)} />
  <CompactMetric label={<ArrowUp />} value={renderSpeed(net_out_speed)} />
  <CompactMetric label="R/W" value={`${renderSpeed(disk_r)} · ${renderSpeed(disk_w)}`} />
  <CompactMetric label="load" value={`${load5.toFixed(2)}·${load15.toFixed(2)}`} />
</div>
```

`CompactMetric` 不变,只是从 5 列改成 2 列。R/W 合并显示节省横向空间。

### 6. 网络质量行 (核心改动)

**数据**: `buildServerCardNetworkState` 已经返回 `latencyPoints` 和 `lossPoints` 两个独立数组,无需修改数据层接口。仅放宽常量:

```ts
// server-card-network-data.ts
- const MAX_TREND_POINTS = 12
+ const MAX_TREND_POINTS = 30
```

(若上游 history bucket 数不足 30 个,`padState` 会用 synthetic 点补齐。视觉上 synthetic 点用空方块或暗色表示。)

**渲染**: 新增组件 `NetworkSquareGrid`,左右各放一个。

```tsx
<div className="grid grid-cols-2 gap-x-3 gap-y-1">
  <NetworkMetricHeader icon={<Clock />} label={t('card_latency')} value={fmtLatency(currentAvgLatency)} />
  <NetworkMetricHeader icon={<SignalOff />} label={t('card_packet_loss')} value={fmtLoss(currentAvgLossRatio)} />
  <NetworkSquareGrid points={latencyPoints} kind="latency" />
  <NetworkSquareGrid points={lossPoints} kind="loss" />
</div>
```

`NetworkSquareGrid` 实现:

```tsx
const SQUARE_SIZE = 6
const SQUARE_GAP = 2
const STEP = SQUARE_SIZE + SQUARE_GAP  // 8px

function NetworkSquareGrid({ points, kind }: { points: ServerCardMetricPoint[]; kind: 'latency' | 'loss' }) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)

  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? 0
      setWidth(w)
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  const maxSquares = Math.max(1, Math.floor((width + SQUARE_GAP) / STEP))
  const visible = points.slice(-maxSquares)

  return (
    <div ref={containerRef} className="flex h-3 items-end gap-[2px] overflow-hidden">
      {visible.map((point, i) => (
        <Tooltip key={i} content={<TooltipContent point={point} t={t} />}>
          <div
            className="h-[6px] w-[6px] flex-none rounded-[1px]"
            style={{ backgroundColor: getSquareColor(point, kind) }}
          />
        </Tooltip>
      ))}
    </div>
  )
}
```

颜色规则 `getSquareColor`:

- **latency 栅格**:
  - synthetic / value=null → `bg-muted` (灰)
  - 失败(对应 loss 100%)→ 红
  - <80ms → emerald
  - 80~150ms → 黄
  - 150~300ms → 橙
  - ≥300ms → 红
- **loss 栅格**:
  - synthetic → 灰
  - lossRatio=0 → emerald
  - 0~1% → 浅绿
  - 1~5% → 黄/橙
  - >5% → 红

具体阈值与现有 `network-latency-constants` 对齐,把现有的 `getCombinedBarColor` 拆成 `getLatencySquareColor` / `getLossSquareColor` 两个纯函数。

**Tooltip**: 复用现有 `NetworkChartTooltip` 的 target 明细渲染逻辑,把它从依赖 recharts payload 改成接受 `ServerCardMetricPoint` 直接渲染。

**SeverityBar 与 BarChart 退役**: 不再被 ServerCard 引用。组件文件保留(不删除)以避免别处引用断裂;如果通过 search 确认没有引用,后续清理任务可以删。

### 7. Misc 行 + Cost footnote

保持现状逻辑与文案,只把 `text-[10px]` 与 `gap-x-3 gap-y-0.5` 维持。

### 8. Tags chips 行 (新)

```tsx
{server.tags && server.tags.length > 0 && (
  <div className="flex flex-wrap gap-1">
    {server.tags.map((tag) => (
      <span
        key={tag}
        className="rounded-md border border-emerald-500/30 bg-emerald-500/5 px-1.5 py-0.5 text-[10px] text-emerald-700 dark:text-emerald-300"
      >
        {tag}
      </span>
    ))}
  </div>
)}
```

如果 `tags` 为空或未定义,整行不渲染。

## 数据/接口变更

- `RingChart` 增加可选 `compact?: boolean` prop (或改用固定尺寸容器策略)
- `MAX_TREND_POINTS` 从 12 改为 30
- 拆分 `network-latency-constants` 里的 `getCombinedBarColor` 为 `getLatencySquareColor` / `getLossSquareColor`
- 新增内部辅助组件: `RingMetric` / `NetworkSquareGrid` / `NetworkMetricHeader` / `TagChips` (均放在 `server-card.tsx` 同目录,内部使用,无需导出)

## 测试

- 现有 `server-card.test.tsx` 需要更新断言: 不再断言 "BarChart" / "SeverityBar",改成断言每个方块栅格的 dom 结构 + tooltip 展示
- 新增测试: `NetworkSquareGrid` 在不同容器宽度下渲染对应数量的方块 (使用 mock `ResizeObserver`)
- 现有 `server-card-network-data.test.ts` 中 `MAX_TREND_POINTS` 相关期望需要从 12 改成 30 (或重新设计成不依赖固定常量的断言)
- 视觉验证: `bun run dev` 在三档视口 (1280 / 1562 / 1920) 下确认每行 column 数自动伸缩;空 tags / 离线状态 / loss=100% (失败标记) / synthetic-only 数据 几种 edge case 视觉正常

## 不在本次范围

- IPv4/IPv6 badge (需扩 WS 协议带 `ipv4` / `ipv6` 字段,作为独立任务)
- ISP tag 的自动识别 (本次只渲染用户手工 tag)
- Widget config dialog 中 columns 字段的废弃/隐藏 (本次只让 ServerCardsWidget 忽略它)
- 删除遗留的 SeverityBar / BarChart 引用与代码 (本次只让 ServerCard 不再使用)

## 风险与权衡

- **`config.columns` 行为变更**: 已设置自定义 columns 的用户会看到 layout 跟以前不一致。可以接受,因为 320~480px 区间是合理可控的视觉结果;若必须保留旧行为,可加一个 feature flag,但增加复杂度,本次不做。
- **`MAX_TREND_POINTS` 放宽到 30**: history bucket 数据量加大可能影响 props diff / memo。`severityPoints` 的 `useMemo` 已经依赖 `latencyPoints`,引用稳定即可;新增 `NetworkSquareGrid` 内部用 `slice(-N)` 重新计算但是数组小、成本可忽略。
- **方块尺寸固定 6px**: 高 DPR 显示器下可能略小。如果反馈不够清晰可考虑改为 8px + 3px gap。
