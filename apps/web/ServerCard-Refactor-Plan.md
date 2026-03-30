# ServerCard 重构实施计划

## 1. 项目概述

重构 ServerCard 组件以显示更多机器信息和网络质量图表，提高信息密度同时保持视觉层次清晰。

## 2. 设计系统分析

### 2.1 颜色系统
- **图表颜色**: `--chart-1` (蓝色), `--chart-2` (绿色), `--chart-3` (黄色), `--chart-4` (红色), `--chart-5` (紫色)
- **语义颜色**: `--muted-foreground` (次要文本), `--foreground` (主要文本), `--border`, `--accent`
- **状态颜色**: `emerald` (在线/良好), `red` (离线/警告), `amber` (注意)

### 2.2 排版系统
- **字体**: Inter Variable
- **大小层级**: text-xs (标签/辅助), text-sm (正文), font-semibold (标题/数值)
- **数值字体**: font-mono tabular-nums (对齐数字)

### 2.3 间距系统
- **卡片内边距**: p-4 (16px)
- **组件间距**: gap-4 (16px), gap-3 (12px), gap-2 (8px), gap-1.5 (6px)
- **紧凑间距**: gap-1 (4px)

## 3. 文件修改列表

### 3.1 新建文件

| 文件路径 | 说明 | 依赖 |
|---------|------|------|
| `apps/web/src/components/ui/sparkline.tsx` | SparklineChart 微型折线图组件 | recharts |
| `apps/web/src/components/server/compact-metric.tsx` | CompactMetric 紧凑指标组件 | 无 |
| `apps/web/src/components/server/server-card-skeleton.tsx` | 卡片加载骨架屏 | Skeleton |

### 3.2 修改文件

| 文件路径 | 修改内容 |
|---------|---------|
| `apps/web/src/components/server/server-card.tsx` | 完全重构，新布局 |
| `apps/web/src/locales/en/servers.json` | 添加新 i18n keys |
| `apps/web/src/locales/zh/servers.json` | 添加新 i18n keys |

## 4. 组件接口设计

### 4.1 SparklineChart 组件

```typescript
interface SparklineChartProps {
  data: number[]                    // 数据点数组
  width?: number                    // 图表宽度 (默认 60)
  height?: number                   // 图表高度 (默认 24)
  color?: string                     // 线条颜色 (默认 'var(--color-chart-1)')
  fillColor?: string                 // 填充颜色 (默认带透明度)
  strokeWidth?: number               // 线条宽度 (默认 1.5)
  showArea?: boolean                 // 是否显示填充区域 (默认 true)
}
```

**设计决策**:
- 使用 `AreaChart` 而非 `LineChart` 以获得更好的视觉效果
- 禁用所有坐标轴、网格、提示框，保持极简
- 使用 `monotone` 曲线类型使线条更平滑
- 固定宽高比确保一致性

### 4.2 CompactMetric 组件

```typescript
interface CompactMetricProps {
  label: string                     // 标签文本
  value: string | number             // 主要数值
  subValue?: string                  // 次要数值/单位
  icon?: React.ReactNode             // 可选图标
  trend?: 'up' | 'down' | 'neutral' // 趋势指示
  className?: string                 // 额外样式
}
```

**设计决策**:
- 垂直布局: 标签在上，数值在下
- 标签使用 `text-xs text-muted-foreground`
- 数值使用 `text-sm font-semibold`
- 可选图标使用 `size-3.5 text-muted-foreground`

### 4.3 ServerCard 组件 (重构后)

```typescript
interface ServerCardProps {
  server: ServerMetrics
  networkData?: NetworkProbeResultData[]  // 可选网络质量数据
}
```

## 5. 布局结构草图

### 5.1 新 ServerCard 布局 (文字描述)

```
┌─────────────────────────────────────────────────────────┐
│ [🇨🇳] [🐧] Server Name                    [Online]     │  ← Header Row
├─────────────────────────────────────────────────────────┤
│                                                         │
│  CPU: 45%    Memory: 62%    Disk: 78%                  │  ← Progress Bars Row
│  ████████    ██████████    ████████████              │
│                                                         │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Load: 1.2   Processes: 42  TCP: 15  UDP: 8          │  ← System Metrics Grid
│  Swap: 2GB   Uptime: 5d 3h                              │
│                                                         │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ↓ 1.2 MB/s  │  Latency: ██📈 45ms  Loss: ██📉 0.1%   │  ← Network Row
│  ↑ 0.8 MB/s  │  [Sparkline]        [Sparkline]       │
│  Total: 1.5TB│                                        │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 5.2 布局层次

1. **Header Section** (flex justify-between)
   - 左侧: 国旗 + OS图标 + 服务器名称 (truncate)
   - 右侧: 状态徽章

2. **Resource Bars Section** (flex flex-col gap-2)
   - CPU 进度条 (chart-1)
   - Memory 进度条 (chart-2)
   - Disk 进度条 (chart-3)

3. **System Metrics Grid** (grid grid-cols-4 gap-2)
   - 负载 (load1)
   - 进程数 (process_count)
   - TCP连接 (tcp_conn)
   - UDP连接 (udp_conn)
   - Swap使用 (swap_used/swap_total)
   - 运行时间 (uptime)

4. **Network Section** (flex justify-between items-end)
   - 左侧: 网络速度 + 总流量 (垂直堆叠)
   - 右侧: 网络质量图表 (延迟 + 丢包率 sparklines)

## 6. i18n Keys 需要添加

### 6.1 英文 (en/servers.json)

```json
{
  "card_load": "Load",
  "card_processes": "Processes",
  "card_tcp": "TCP",
  "card_udp": "UDP",
  "card_swap": "Swap",
  "card_latency": "Latency",
  "card_packet_loss": "Loss",
  "card_net_total": "Total",
  "card_network_quality": "Network Quality"
}
```

### 6.2 中文 (zh/servers.json)

```json
{
  "card_load": "负载",
  "card_processes": "进程",
  "card_tcp": "TCP",
  "card_udp": "UDP",
  "card_swap": "Swap",
  "card_latency": "延迟",
  "card_packet_loss": "丢包",
  "card_net_total": "总流量",
  "card_network_quality": "网络质量"
}
```

## 7. 并行任务分解

### 任务 A: SparklineChart 组件 (独立)
- **文件**: `apps/web/src/components/ui/sparkline.tsx`
- **依赖**: 无 (仅 recharts)
- **验收标准**:
  - [ ] 组件接受 data: number[] 属性
  - [ ] 渲染 AreaChart 无坐标轴
  - [ ] 支持自定义颜色和尺寸
  - [ ] 在 Storybook 或测试页面可预览

### 任务 B: CompactMetric 组件 (独立)
- **文件**: `apps/web/src/components/server/compact-metric.tsx`
- **依赖**: 无
- **验收标准**:
  - [ ] 正确显示 label 和 value
  - [ ] 支持可选 subValue 和 icon
  - [ ] 使用正确的文本样式 (text-xs label, text-sm value)
  - [ ] 响应式布局正常

### 任务 C: i18n 翻译 (独立)
- **文件**: `apps/web/src/locales/en/servers.json`, `apps/web/src/locales/zh/servers.json`
- **依赖**: 无
- **验收标准**:
  - [ ] 所有新 keys 已添加
  - [ ] 中英文翻译完整
  - [ ] 命名一致 (card_ 前缀)

### 任务 D: ServerCard 重构 (依赖 A, B, C)
- **文件**: `apps/web/src/components/server/server-card.tsx`
- **依赖**: SparklineChart, CompactMetric, i18n keys
- **验收标准**:
  - [ ] 新布局实现所有区域 (Header, Resources, System, Network)
  - [ ] 显示所有计划中的指标
  - [ ] 网络质量图表集成 (使用 SparklineChart)
  - [ ] 保持可点击跳转功能
  - [ ] 响应式布局 (grid 视图兼容)
  - [ ] 视觉层次清晰 (主次信息区分)
  - [ ] 使用语义化颜色
  - [ ] 通过 TypeScript 检查
  - [ ] 通过 Biome lint

## 8. 实施顺序

```
┌─────────────┐   ┌─────────────┐   ┌─────────────┐
│  Task A     │   │  Task B     │   │  Task C     │
│ Sparkline   │   │ CompactMetric│   │   i18n      │
│   Chart     │   │             │   │             │
└──────┬──────┘   └──────┬──────┘   └──────┬──────┘
       │                 │                 │
       └─────────────────┴─────────────────┘
                         │
                         ▼
               ┌─────────────────┐
               │    Task D       │
               │  ServerCard     │
               │   重构          │
               └─────────────────┘
```

## 9. 技术细节

### 9.1 SparklineChart 实现要点

```tsx
// 使用 Recharts AreaChart
<AreaChart width={width} height={height} data={chartData}>
  <Area
    type="monotone"
    dataKey="value"
    stroke={color}
    fill={fillColor}
    strokeWidth={strokeWidth}
  />
</AreaChart>

// 数据转换
const chartData = data.map((value, index) => ({ value, index }))
```

### 9.2 网络质量数据获取

```tsx
// 在 ServerCard 中使用 useNetworkRealtime hook
const { data: networkData } = useNetworkRealtime(server.id)

// 聚合数据用于 sparkline
const latencyData = networkData[targetId]?.map(r => r.avg_latency).filter(Boolean) || []
const lossData = networkData[targetId]?.map(r => r.packet_loss * 100) || []
```

### 9.3 响应式考虑

- 卡片在 grid 视图中使用 `sm:grid-cols-2 lg:grid-cols-3`
- 卡片内部使用 flex 和 grid 布局
- 文本使用 truncate 防止溢出
- 网络质量区域在小屏幕可隐藏或简化

## 10. 验收标准汇总

### 功能验收
- [ ] 所有 ServerMetrics 字段正确显示
- [ ] 网络质量图表实时更新
- [ ] 点击卡片正确跳转到详情页
- [ ] 响应式布局在各种屏幕尺寸正常

### 设计验收
- [ ] 使用设计系统颜色变量
- [ ] 视觉层次清晰 (主信息突出，次信息弱化)
- [ ] 间距一致 (使用 gap-1, gap-2, gap-3, gap-4)
- [ ] 文本样式符合规范

### 代码质量
- [ ] TypeScript 无错误
- [ ] Biome lint 通过
- [ ] 组件接口文档完整
- [ ] i18n 翻译完整

## 11. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|-----|------|---------|
| 信息密度过高 | 中 | 提供紧凑/详细视图切换 |
| 网络图表性能 | 低 | 限制数据点数量 (MAX_POINTS) |
| 小屏幕显示 | 中 | 响应式隐藏次要信息 |
| 颜色对比度 | 低 | 使用语义化颜色，测试暗色模式 |
