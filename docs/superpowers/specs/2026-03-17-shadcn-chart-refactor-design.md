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

| 系列 | 颜色来源 | 值 |
|------|----------|-----|
| 1-5 | shadcn CSS 变量 | `var(--chart-1)` ~ `var(--chart-5)` |
| 6 | Tailwind blue-500 | `#3b82f6` |
| 7 | Tailwind emerald-500 | `#10b981` |
| 8 | Tailwind amber-500 | `#f59e0b` |
| 9 | Tailwind rose-500 | `#f43f5e` |
| 10 | Tailwind cyan-500 | `#06b6d4` |
| 11 | Tailwind lime-500 | `#84cc16` |
| 12 | Tailwind pink-500 | `#ec4899` |

只有 LatencyChart 的多目标场景会用到 6-12，其余图表最多 5 个系列。

### 3. MetricsChart 重构

通用 AreaChart 包装器，被服务器详情页调用 9 次。

**核心变化**：

- `ResponsiveContainer` + 手动高度 → `ChartContainer` + `className="h-[200px]"`
- 删除 `<defs><linearGradient>` — 用 `fillOpacity={0.1}` 替代渐变
- 删除所有手动 `stroke`/`tick`/`contentStyle` — ChartContainer 自动注入主题色
- 组件 props 不变（`data`, `dataKey`, `label`, `color`, `formatter` 等），调用方无需改动

**重构后结构**：

```tsx
const chartConfig = {
  [dataKey]: { label, color }
} satisfies ChartConfig

<ChartContainer config={chartConfig} className="h-[200px] w-full">
  <AreaChart data={data}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="timestamp" tickFormatter={formatTime} tickLine={false} axisLine={false} />
    <YAxis tickLine={false} axisLine={false} />
    <ChartTooltip content={<ChartTooltipContent />} />
    <Area type="monotone" dataKey={dataKey} stroke="var(--color-{dataKey})"
      fill="var(--color-{dataKey})" fillOpacity={0.1} />
  </AreaChart>
</ChartContainer>
```

### 4. LatencyChart 重构

最复杂的图表：多目标系列、60 秒时间桶聚合、动态 tick 间隔。

**核心变化**：

- 硬编码 12 色数组 → `COLORS` 常量：前 5 个 `"var(--chart-N)"`，后 7 个 Tailwind 色值
- 手动 Tooltip 样式 → `ChartTooltipContent`（自动显示标签 + 颜色指示器）
- 新增 `ChartLegend` 展示
- 时间桶逻辑、动态 tick 计算 — 完全不动

**重构后结构**：

```tsx
const chartConfig = useMemo(() => {
  const config: ChartConfig = {}
  targets.forEach((target, i) => {
    config[target.id] = {
      label: target.name,
      color: COLORS[i % COLORS.length],
    }
  })
  return config
}, [targets])

<ChartContainer config={chartConfig} className="h-[300px] w-full">
  <AreaChart data={bucketedData}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="timestamp" tickFormatter={formatTime} ... />
    <YAxis unit=" ms" ... />
    <ChartTooltip content={<ChartTooltipContent />} />
    <ChartLegend content={<ChartLegendContent />} />
    {targets.map((target) => (
      <Area key={target.id} dataKey={target.id}
        stroke={`var(--color-${target.id})`}
        fill={`var(--color-${target.id})`}
        fillOpacity={0.05} />
    ))}
  </AreaChart>
</ChartContainer>
```

### 5. TrafficCard 重构

两个子图表共享同一个 `trafficConfig`。

**核心变化**：

- `hsl(var(--chart-N))` → `var(--chart-N)`（shadcn 标准引用方式）
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
  <BarChart data={daily}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="date" ... />
    <YAxis tickFormatter={formatBytes} ... />
    <ChartTooltip content={<ChartTooltipContent formatter={formatBytes} />} />
    <ChartLegend content={<ChartLegendContent />} />
    <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={4} stackId="traffic" />
    <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={4} stackId="traffic" />
  </BarChart>
</ChartContainer>

// 小时流量 LineChart
<ChartContainer config={trafficConfig} className="h-[160px] w-full">
  <LineChart data={hourly}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="hour" ... />
    <YAxis tickFormatter={formatBytes} ... />
    <ChartTooltip content={<ChartTooltipContent formatter={formatBytes} />} />
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
- `connectNulls={false}` 保留（ping 失败时断线是有意义的视觉反馈）

**重构后结构**：

```tsx
const pingChartConfig = {
  latency: { label: 'Latency', color: 'var(--chart-4)' },
} satisfies ChartConfig

<ChartContainer config={pingChartConfig} className="h-[180px] w-full">
  <AreaChart data={results}>
    <CartesianGrid vertical={false} />
    <XAxis dataKey="timestamp" tickFormatter={formatTime} tickLine={false} axisLine={false} />
    <YAxis unit=" ms" tickLine={false} axisLine={false} />
    <ChartTooltip content={<ChartTooltipContent />} />
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
| `components/server/traffic-card.tsx` | 重构 |
| `routes/_authed/settings/ping-tasks.tsx` | 重构内联图表部分 |
| `index.css` | 删除 recharts outline hack |

## 不做的事

- 不动数据层（hooks、API client、WebSocket）
- 不动图表的业务逻辑（时间桶聚合、tick 计算、数据格式化）
- 不改调用方（页面组件中使用图表的代码尽量不改，props 兼容）
- 不动 CSS 变量值（`--chart-1` 到 `--chart-5` 的 oklch 值保持不变）
- 不升降 Recharts 版本

## 风险

- shadcn Chart 官方依赖 Recharts 2.x，当前项目用 3.x。API 基本兼容，但生成的 `chart.tsx` 如有 2.x 特有用法需要微调。
