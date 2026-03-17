# shadcn Chart 图表重构设计

## 目标

将前端 4 个图表文件从 Recharts 直接使用迁移到 shadcn/ui Chart 封装层，提升视觉效果，采用 shadcn Chart 的默认风格（圆角 tooltip、内置 legend、主题色自动适配）。

## 背景

当前项目使用 Recharts v3.8.0 直接构建图表，4 个图表文件各自重复配置 Tooltip 样式、轴样式、网格样式。shadcn/ui 提供了基于 Recharts 的 Chart 封装组件，可以统一这些样式并提供更好的默认视觉效果。

### 当前图表文件

| 文件 | 图表类型 | 用途 |
|------|----------|------|
| `components/server/metrics-chart.tsx` | AreaChart | 系统指标（CPU/内存/磁盘/网速/负载/温度/GPU） |
| `components/network/latency-chart.tsx` | AreaChart (多系列) | 网络探测延迟（最多 12 个目标） |
| `components/server/traffic-card.tsx` | BarChart + LineChart | 日流量 + 小时流量 |
| `routes/_authed/settings/ping-tasks.tsx` | AreaChart (内联) | Ping 任务延迟趋势 |

## 设计

### 1. 安装 shadcn Chart 组件

通过 `bunx shadcn@latest add chart` 生成 `components/ui/chart.tsx`，提供：

- `ChartContainer` — 替代 `ResponsiveContainer`，自动注入 CSS 变量
- `ChartTooltip` + `ChartTooltipContent` — 统一 tooltip 风格
- `ChartLegend` + `ChartLegendContent` — 统一图例
- `ChartConfig` 类型 — 声明式颜色/标签管理

### 2. 颜色体系

**单系列图表**（MetricsChart、PingResultsChart、TrafficCard）：直接使用 shadcn CSS 变量 `var(--chart-1)` ~ `var(--chart-5)`，最多 5 个系列，颜色区分足够。

**多系列图表**（LatencyChart，最多 12 个目标）：使用独立的高区分度多色盘 `COLORS`，**不使用** `--chart-1..5`（因为这些变量是同一组蓝紫色，色相不够分散）。保留当前 `COLOR_PALETTE` 的色相分散特性：

| 序号 | 颜色 | 值 |
|------|------|-----|
| 1 | blue-500 | `#3b82f6` |
| 2 | red-500 | `#ef4444` |
| 3 | green-500 | `#22c55e` |
| 4 | amber-500 | `#f59e0b` |
| 5 | violet-500 | `#8b5cf6` |
| 6 | pink-500 | `#ec4899` |
| 7 | teal-500 | `#14b8a6` |
| 8 | orange-500 | `#f97316` |
| 9 | indigo-500 | `#6366f1` |
| 10 | cyan-500 | `#06b6d4` |
| 11 | lime-500 | `#84cc16` |
| 12 | rose-600 | `#e11d48` |

这与当前 `$serverId.tsx:33` 的 `COLOR_PALETTE` 完全一致，确保迁移后视觉效果不退化。

### 3. MetricsChart 重构

通用 AreaChart 包装器，被服务器详情页调用 6-9 次（6 个固定指标 + 温度/GPU 按硬件条件显示）。

**核心变化**：

- `ResponsiveContainer` + 手动高度 → `ChartContainer` + `className="h-[200px]"`
- 删除 `<defs><linearGradient>` — 用 `fillOpacity={0.1}` 替代渐变
- 删除手动 `stroke`/`tick`/`contentStyle` — ChartContainer 自动注入主题色
- **保留 `formatValue` 和 `formatTime` props** — 当前调用方传入自定义 formatter（如 `formatSpeed` 格式化网速、温度单位 `°C` 等），通过 `ChartTooltipContent` 的 `formatter` 和 `labelFormatter` prop 传递：`formatter={(value) => \`${formatValue(Number(value))}${unit}\`}`、`labelFormatter={(label) => formatTime(String(label))}`
- 组件 props 不变（`data`, `dataKey`, `title`, `color`, `unit`, `formatValue`, `formatTime`），调用方无需改动

**注意**：`dataKey` 值（`cpu`, `memory_pct`, `disk_pct` 等）都是合法的 CSS 自定义属性名片段，可安全用作 ChartConfig key。

**重构后结构**：

```tsx
const chartConfig = {
  [dataKey]: { label, color }
} satisfies ChartConfig

<ChartContainer config={chartConfig} className="h-[200px] w-full">
  <AreaChart accessibilityLayer data={data}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="timestamp" tickFormatter={formatTime} tickLine={false} axisLine={false} />
    <YAxis tickLine={false} axisLine={false} />
    <ChartTooltip content={<ChartTooltipContent
      formatter={(value) => `${formatValue(Number(value))}${unit}`}
      labelFormatter={(label) => formatTime(String(label))}
    />} />
    <Area type="monotone" dataKey={dataKey} stroke={`var(--color-${dataKey})`}
      fill={`var(--color-${dataKey})`} fillOpacity={0.1} />
  </AreaChart>
</ChartContainer>
```

组件外部包裹的 `<div className="rounded-lg border bg-card p-4">` 和 `<h3>` 标题保持不变。

### 4. LatencyChart 重构

最复杂的图表：多目标系列、60 秒时间桶聚合、动态 tick 间隔。

**核心变化**：

- 硬编码 12 色数组 → 共享 `COLORS` 常量（与当前 `COLOR_PALETTE` 值完全一致，见上方颜色体系）
- 手动 Tooltip 样式 → `ChartTooltipContent`，但需保留自定义 formatter（`value.toFixed(1) ms`）和 labelFormatter（日期时间格式化），通过 `ChartTooltipContent` 的 `formatter` 和 `labelFormatter` prop 传入
- **不加 `ChartLegend`** — 网络页已有可点击的 TargetCard 充当图例（支持隐藏/显示目标），再加 ChartLegend 会功能重复且无法联动
- **保留 `visibleTargets` 过滤**：当前 LatencyChart 接收 `targets: TargetInfo[]`（含 `visible` 字段），内部用 `targets.filter(t => t.visible)` 只渲染可见目标的 Area 系列。重构后必须保留此逻辑，chartConfig 包含所有目标（用于颜色注入），但 Area 系列只渲染 visibleTargets
- 时间桶逻辑、动态 tick 计算 — 完全不动
- **颜色分配统一**：`COLORS` 数组同时用于 LatencyChart 的 ChartConfig 和父组件 `$serverId.tsx` 的 TargetCard 颜色，需将 `COLORS` 导出为共享常量，替换父组件中原有的 `COLOR_PALETTE`，确保图表线条与目标卡片颜色一致

**注意**：ChartConfig 的 key 使用 `target_${index}` 形式而非 `target.id`（因为 target.id 可能是 UUID，虽然技术上可作为 CSS 自定义属性名，但不够简洁）。对应地，数据转换时也使用 `target_${index}` 作为 dataKey。

**重构后结构**：

```tsx
// COLORS 从共享常量导入，父组件 TargetCard 也使用同一数组
// chartConfig 包含所有目标（ChartContainer 需注入所有颜色变量）
const chartConfig = useMemo(() => {
  const config: ChartConfig = {}
  targets.forEach((target, i) => {
    config[`target_${i}`] = {
      label: target.name,
      color: COLORS[i % COLORS.length],
    }
  })
  return config
}, [targets])

// 只渲染可见目标的 Area 系列
const visibleTargets = useMemo(() => targets.filter((t) => t.visible), [targets])
// 需要 index 映射：visibleTarget → 在 targets 中的原始 index（用于 dataKey）
const visibleWithIndex = useMemo(
  () => visibleTargets.map((t) => ({ ...t, originalIndex: targets.indexOf(t) })),
  [targets, visibleTargets]
)

<ChartContainer config={chartConfig} className="h-[300px] w-full">
  <AreaChart accessibilityLayer data={bucketedData}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="timestamp" tickFormatter={formatTime} ... />
    <YAxis unit=" ms" ... />
    <ChartTooltip content={<ChartTooltipContent
      formatter={(value) => `${Number(value).toFixed(1)} ms`}
      labelFormatter={(label) => new Date(label).toLocaleString([], {
        month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit'
      })}
    />} />
    {visibleWithIndex.map(({ id, originalIndex }) => (
      <Area key={id} dataKey={`target_${originalIndex}`}
        stroke={`var(--color-target_${originalIndex})`}
        fill={`var(--color-target_${originalIndex})`}
        fillOpacity={0.05} connectNulls={false} type="monotone" strokeWidth={2} />
    ))}
  </AreaChart>
</ChartContainer>
```

### 5. TrafficCard 重构

两个子图表共享同一个 `trafficConfig`。

**核心变化**：

- `hsl(var(--chart-N))` → `var(--chart-N)`（顺带修复已有 bug：`--chart-N` 变量值是 oklch 格式，用 `hsl()` 包裹是无效 CSS）
- 新增 `ChartLegend`（仅 BarChart，LineChart 空间有限不加）
- Bar 加 `radius={4}` 圆角
- 删除手动 Tooltip 样式

**重构后结构**：

```tsx
const trafficConfig = {
  bytes_in: { label: t('traffic.in', '↓ In'), color: 'var(--chart-1)' },
  bytes_out: { label: t('traffic.out', '↑ Out'), color: 'var(--chart-2)' },
} satisfies ChartConfig

// 日流量 BarChart
<ChartContainer config={trafficConfig} className="h-[200px] w-full">
  <BarChart accessibilityLayer data={daily}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="date" ... />
    <YAxis tickFormatter={formatBytes} ... />
    <ChartTooltip content={<ChartTooltipContent formatter={(value) => formatBytes(Number(value))} />} />
    <ChartLegend content={<ChartLegendContent />} />
    <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={4} stackId="traffic" />
    <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={4} stackId="traffic" />
  </BarChart>
</ChartContainer>

// 小时流量 LineChart
<ChartContainer config={trafficConfig} className="h-[160px] w-full">
  <LineChart accessibilityLayer data={hourly}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="hour" ... />
    <YAxis tickFormatter={formatBytes} ... />
    <ChartTooltip content={<ChartTooltipContent formatter={(value) => formatBytes(Number(value))} />} />
    <Line type="monotone" dataKey="bytes_in" stroke="var(--color-bytes_in)" dot={false} />
    <Line type="monotone" dataKey="bytes_out" stroke="var(--color-bytes_out)" dot={false} />
  </LineChart>
</ChartContainer>
```

### 6. PingResultsChart 重构

内联在 `ping-tasks.tsx` 中，保持内联不提取。

**核心变化**：

- `ResponsiveContainer` → `ChartContainer`
- 删除手动渐变定义
- **保留 tooltip formatter**：当前 formatter 显示 `${value.toFixed(1)}ms`，labelFormatter 显示完整日期时间。需通过 `ChartTooltipContent` 的 prop 传入
- `connectNulls={false}` 保留（ping 失败时断线是有意义的视觉反馈）

**重构后结构**：

```tsx
const pingChartConfig = {
  latency: { label: 'Latency', color: 'var(--chart-4)' },
} satisfies ChartConfig

<ChartContainer config={pingChartConfig} className="h-[180px] w-full">
  <AreaChart accessibilityLayer data={results}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="timestamp" tickFormatter={formatTime} tickLine={false} axisLine={false} />
    <YAxis unit=" ms" tickLine={false} axisLine={false} />
    <ChartTooltip content={<ChartTooltipContent
      formatter={(value) => `${Number(value).toFixed(1)}ms`}
      labelFormatter={(label) => new Date(String(label)).toLocaleString()}
    />} />
    <Area type="monotone" dataKey="latency" stroke="var(--color-latency)"
      fill="var(--color-latency)" fillOpacity={0.1} connectNulls={false} />
  </AreaChart>
</ChartContainer>
```

### 7. CSS 清理

- 保留 `index.css` 中已有的 `--chart-1` 到 `--chart-5` 变量
- 删除 `.recharts-wrapper, .recharts-wrapper * { outline: none !important; }` hack

## 变更清单

| 文件 | 操作 |
|------|------|
| `components/ui/chart.tsx` | 新增（shadcn 生成） |
| `components/server/metrics-chart.tsx` | 重构 |
| `components/network/latency-chart.tsx` | 重构 |
| `components/server/traffic-card.tsx` | 重构（顺带修复 `hsl(oklch(...))` bug） |
| `routes/_authed/settings/ping-tasks.tsx` | 重构内联图表部分 |
| `routes/_authed/network/$serverId.tsx` | 更新 COLOR_PALETTE → 共享 COLORS 常量 |
| `index.css` | 删除 recharts outline hack（需视觉验证 ChartContainer 是否已处理） |

## 不做的事

- 不动数据层（hooks、API client、WebSocket）
- 不动图表的业务逻辑（时间桶聚合、tick 计算、数据格式化）
- 不改调用方（页面组件中使用图表的代码尽量不改，props 兼容；例外：`$serverId.tsx` 的 COLOR_PALETTE 需同步更新为共享 COLORS 常量）
- 不动 CSS 变量值（`--chart-1` 到 `--chart-5` 的 oklch 值保持不变）
- 不升降 Recharts 版本

## 验证清单

实现完成后须逐项人工验证：

- [ ] **目标隐藏/显示**：网络详情页点击 TargetCard 切换可见性，图表中对应线条正确隐藏/显示
- [ ] **Tooltip 格式化**：MetricsChart 的网速显示为 `xx.x MB/s` 而非原始数字，温度显示 `°C` 后缀
- [ ] **Tooltip 格式化**：PingResultsChart 显示 `xx.xms` 而非原始数字，label 显示完整日期时间
- [ ] **Tooltip 格式化**：TrafficCard 显示 `xx.x MB` 等格式化后的字节数
- [ ] **Tooltip 格式化**：LatencyChart 显示 `xx.x ms` 和目标名称（非 target_id）
- [ ] **浅色主题**：所有图表在浅色模式下颜色、背景、文字对比度正常
- [ ] **深色主题**：所有图表在深色模式下颜色、背景、文字对比度正常
- [ ] **TrafficCard 展开态**：服务器详情页点击展开 TrafficCard，日流量 BarChart 和小时流量 LineChart 正常渲染
- [ ] **Ping 失败断线**：PingResultsChart 中 ping 失败的时间点显示为断线而非连续
- [ ] **颜色一致性**：网络详情页 TargetCard 颜色圆点与图表线条颜色一一对应
- [ ] **recharts outline**：删除 CSS hack 后图表元素无多余 outline/focus ring

## 风险

- shadcn Chart 官方依赖 Recharts 2.x，当前项目用 3.x。API 基本兼容，但生成的 `chart.tsx` 如有 2.x 特有用法需要微调。
