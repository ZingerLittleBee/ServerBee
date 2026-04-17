# Server Card 延迟/丢包紧凑展示实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `ServerCard` 的延迟展示区从"柱 + 细条 + 小号数字"改造为"紧凑头部 + 合并严重度柱"布局，去掉 `h-1` 丢包细条，失败柱满格 + 斜线。

**Architecture:** 新增一个合并严重度辅助函数（`getCombinedSeverity`），让柱体颜色同时考虑延迟和丢包；用 Recharts 的自定义 `shape` prop 渲染失败柱（value=0/null 但需要满格），通过 SVG `<defs><pattern>` 提供斜线纹理。头部用 flex 两端对齐的紧凑布局，延迟 18px 大字 + 丢包 11px 副文 + 状态圆点。

**Tech Stack:** React 19, TypeScript, Recharts v3, shadcn/ui Chart, Tailwind v4, Vitest

**设计文档:** `docs/superpowers/specs/2026-04-18-server-card-latency-compact-design.md`

---

## File Structure

| 文件 | 责任 | 动作 |
|------|------|------|
| `apps/web/src/lib/network-latency-constants.ts` | 严重度枚举、阈值常量、颜色映射 | 修改（扩展） |
| `apps/web/src/lib/network-latency-constants.test.ts` | 上述函数的单元测试 | 修改（新增 case） |
| `apps/web/src/components/server/server-card.tsx` | 服务器卡片主组件 | 修改（头部 DOM + 柱图） |
| `apps/web/src/components/server/severity-bar.tsx` | Recharts 自定义 shape 组件，渲染严重度柱（失败柱满格 + 斜线） | 新建 |
| `apps/web/src/locales/{zh,en}/servers.json` | i18n 文案 | 修改（清理无用 key） |

---

## Task 1: 扩展 `network-latency-constants.ts` 增加合并严重度函数

**Files:**
- Modify: `apps/web/src/lib/network-latency-constants.ts`
- Test: `apps/web/src/lib/network-latency-constants.test.ts`

- [ ] **Step 1: 在测试文件最下方增加 `getCombinedSeverity` 的失败测试**

打开 `apps/web/src/lib/network-latency-constants.test.ts`，在最后一个 `it(...)` 之后、`describe` 闭合之前，插入：

```ts
  describe('getCombinedSeverity', () => {
    it('returns healthy when latency < 300 and loss < 1%', () => {
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0 })).toBe('healthy')
      expect(getCombinedSeverity({ latencyMs: 299, lossRatio: 0.009 })).toBe('healthy')
    })

    it('returns warning when latency >= 300 or loss in [1%, 5%)', () => {
      expect(getCombinedSeverity({ latencyMs: 300, lossRatio: 0 })).toBe('warning')
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0.01 })).toBe('warning')
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0.049 })).toBe('warning')
    })

    it('returns severe when loss >= 5% but not total failure', () => {
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: 0.05 })).toBe('severe')
      expect(getCombinedSeverity({ latencyMs: 500, lossRatio: 0.5 })).toBe('severe')
    })

    it('returns failed when loss ratio hits 100%', () => {
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: 1 })).toBe('failed')
      expect(getCombinedSeverity({ latencyMs: 0, lossRatio: 1 })).toBe('failed')
    })

    it('returns unknown when both inputs are null', () => {
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: null })).toBe('unknown')
    })

    it('tolerates one null input', () => {
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: 0 })).toBe('healthy')
      expect(getCombinedSeverity({ latencyMs: 50, lossRatio: null })).toBe('healthy')
      expect(getCombinedSeverity({ latencyMs: 400, lossRatio: null })).toBe('warning')
      expect(getCombinedSeverity({ latencyMs: null, lossRatio: 0.1 })).toBe('severe')
    })
  })

  describe('getCombinedBarColor', () => {
    it('maps severity levels to expected hex colors', () => {
      expect(getCombinedBarColor({ latencyMs: 50, lossRatio: 0 })).toBe('#10b981')
      expect(getCombinedBarColor({ latencyMs: 400, lossRatio: 0 })).toBe('#f59e0b')
      expect(getCombinedBarColor({ latencyMs: 50, lossRatio: 0.08 })).toBe('#ef4444')
      expect(getCombinedBarColor({ latencyMs: null, lossRatio: 1 })).toBe('#ef4444')
      expect(getCombinedBarColor({ latencyMs: null, lossRatio: null })).toBe('var(--color-muted)')
    })
  })

  describe('getLossDotBgClass', () => {
    it('maps loss ratio to Tailwind bg class', () => {
      expect(getLossDotBgClass(null)).toBe('bg-muted-foreground')
      expect(getLossDotBgClass(0)).toBe('bg-emerald-500')
      expect(getLossDotBgClass(0.009)).toBe('bg-emerald-500')
      expect(getLossDotBgClass(0.01)).toBe('bg-amber-500')
      expect(getLossDotBgClass(0.049)).toBe('bg-amber-500')
      expect(getLossDotBgClass(0.05)).toBe('bg-red-500')
      expect(getLossDotBgClass(1)).toBe('bg-red-500')
    })
  })
```

同时把 import 行扩展为：

```ts
import {
  getCombinedBarColor,
  getCombinedSeverity,
  getLatencyBarColor,
  getLatencyStatus,
  getLatencyTextClass,
  getLossDotBgClass,
  isLatencyFailure,
  LATENCY_HEALTHY_THRESHOLD_MS
} from './network-latency-constants'
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd apps/web && bun run test src/lib/network-latency-constants.test.ts`
Expected: 新 case 全部失败（函数未定义）。

- [ ] **Step 3: 在 `network-latency-constants.ts` 实现函数**

在文件末尾追加：

```ts
export const LOSS_WARNING_THRESHOLD_RATIO = 0.01
export const LOSS_SEVERE_THRESHOLD_RATIO = 0.05

export type CombinedSeverity = 'unknown' | 'healthy' | 'warning' | 'severe' | 'failed'

interface CombinedSeverityInput {
  latencyMs: number | null | undefined
  lossRatio: number | null | undefined
}

export function getCombinedSeverity({ latencyMs, lossRatio }: CombinedSeverityInput): CombinedSeverity {
  if (lossRatio != null && lossRatio >= NETWORK_FAILURE_PACKET_LOSS_RATIO) {
    return 'failed'
  }
  if (lossRatio != null && lossRatio >= LOSS_SEVERE_THRESHOLD_RATIO) {
    return 'severe'
  }
  if (latencyMs == null && lossRatio == null) {
    return 'unknown'
  }
  const latencyWarn = latencyMs != null && latencyMs >= LATENCY_HEALTHY_THRESHOLD_MS
  const lossWarn = lossRatio != null && lossRatio >= LOSS_WARNING_THRESHOLD_RATIO
  if (latencyWarn || lossWarn) {
    return 'warning'
  }
  return 'healthy'
}

export function getCombinedBarColor(input: CombinedSeverityInput): string {
  switch (getCombinedSeverity(input)) {
    case 'healthy':
      return LATENCY_HEALTHY_BAR_COLOR
    case 'warning':
      return LATENCY_WARNING_BAR_COLOR
    case 'severe':
    case 'failed':
      return LATENCY_FAILED_BAR_COLOR
    default:
      return LATENCY_UNKNOWN_BAR_COLOR
  }
}

export function getLossDotBgClass(lossRatio: number | null | undefined): string {
  if (lossRatio == null) {
    return 'bg-muted-foreground'
  }
  if (lossRatio < LOSS_WARNING_THRESHOLD_RATIO) {
    return 'bg-emerald-500'
  }
  if (lossRatio < LOSS_SEVERE_THRESHOLD_RATIO) {
    return 'bg-amber-500'
  }
  return 'bg-red-500'
}
```

- [ ] **Step 4: 再次运行测试确认全绿**

Run: `cd apps/web && bun run test src/lib/network-latency-constants.test.ts`
Expected: 所有 case PASS。

- [ ] **Step 5: 提交**

```bash
git add apps/web/src/lib/network-latency-constants.ts apps/web/src/lib/network-latency-constants.test.ts
git commit -m "feat(web): add combined severity helpers for latency + loss"
```

---

## Task 2: 创建 `SeverityBar` 自定义 Recharts shape 组件

**Files:**
- Create: `apps/web/src/components/server/severity-bar.tsx`

- [ ] **Step 1: 新建 `severity-bar.tsx`**

```tsx
import type { CombinedSeverity } from '@/lib/network-latency-constants'

export interface SeverityBarDatum {
  combinedSeverity: CombinedSeverity
  fillColor: string
  lossRatio: number | null
  value: number | null
}

interface SeverityBarProps {
  x?: number
  y?: number
  width?: number
  height?: number
  background?: { x: number; y: number; width: number; height: number }
  payload?: SeverityBarDatum
  failPatternId: string
}

export function SeverityBar({ x = 0, y = 0, width = 0, height = 0, background, payload, failPatternId }: SeverityBarProps) {
  if (!payload || width <= 0) {
    return null
  }

  const isFailed = payload.combinedSeverity === 'failed'
  const radius = 2

  if (isFailed) {
    const bgY = background?.y ?? y
    const bgHeight = background?.height ?? height
    return (
      <rect
        x={x}
        y={bgY}
        width={width}
        height={bgHeight}
        fill={`url(#${failPatternId})`}
        rx={radius}
        ry={radius}
      />
    )
  }

  const safeHeight = Math.max(height, 2)
  const safeY = y + (height - safeHeight)

  return (
    <rect
      x={x}
      y={safeY}
      width={width}
      height={safeHeight}
      fill={payload.fillColor}
      rx={radius}
      ry={radius}
    />
  )
}
```

说明：
- `background` prop 是 Recharts 传入的 Bar 可用背景矩形（宽高 = 柱允许的最大空间）。失败柱用 `background.y` 和 `background.height` 撑满。
- 正常柱：value 很小导致 height < 2 时，保证最小 2px 可见。
- `failPatternId` 由父组件传入，对应 `<defs>` 中的 `<pattern id=...>`。

- [ ] **Step 2: 类型检查**

Run: `cd apps/web && bun run typecheck`
Expected: 无错误（未使用的导入暂时不会报错，或仅 unused warning）。

- [ ] **Step 3: 提交**

```bash
git add apps/web/src/components/server/severity-bar.tsx
git commit -m "feat(web): add SeverityBar custom shape for latency chart"
```

---

## Task 3: 改造 `server-card.tsx` 头部 + 柱图 + 删除 loss strip

**Files:**
- Modify: `apps/web/src/components/server/server-card.tsx`

- [ ] **Step 1: 更新 import**

把当前的 network-latency-constants 导入：

```ts
import { getLatencyBarColor, isLatencyFailure } from '@/lib/network-latency-constants'
```

替换为：

```ts
import { getCombinedBarColor, getCombinedSeverity, getLossDotBgClass, isLatencyFailure } from '@/lib/network-latency-constants'
```

在同一文件其他导入区加入 `SeverityBar`：

```ts
import { SeverityBar, type SeverityBarDatum } from './severity-bar'
```

`latencyColorClass`（来自 `@/lib/network-types`）保持不变，继续用于头部延迟字颜色和 tooltip 内。
`getLossTextClassName`（文件内函数，L65-76）保持不变，继续用于头部丢包字颜色和 tooltip 内。

- [ ] **Step 2: 删除 `getLossStripColor` 和 `LOSS_STRIP_CONFIG`**

- 删除 L29-31（`const LOSS_STRIP_CONFIG = ...`）
- 删除 L78-89（`function getLossStripColor(...)`）

- [ ] **Step 3: 新增 `getSeverityBarData` 辅助（文件内）**

在 `averageLossRatio` 函数之后插入：

```tsx
function getSeverityBarData(point: ServerCardMetricPoint): SeverityBarDatum {
  const lossRatio = averageLossRatio(point)
  return {
    combinedSeverity: getCombinedSeverity({ latencyMs: point.value, lossRatio }),
    fillColor: getCombinedBarColor({ latencyMs: point.value, lossRatio }),
    lossRatio,
    value: point.value
  }
}
```

这里把每个 point 的严重度和颜色预计算成一个 `SeverityBarDatum`，后续作为 Bar 的 data 传给 Recharts，shape 通过 `payload` 直接读取。

- [ ] **Step 4: 替换头部 DOM（L357-370）**

把原来的：

```tsx
<div className="mb-1 flex items-center justify-between">
  <span className="text-[10px] text-muted-foreground">{t('card_latency')}</span>
  <div className="flex items-center gap-1 font-medium text-xs">
    <span className={latencyColorClass(currentAvgLatency, { failed: isLatencyFailure(currentAvgLossRatio) })}>
      {formatLatency(currentAvgLatency)}
    </span>
    <span className="text-muted-foreground">·</span>
    <span className={getLossTextClassName(currentAvgLossRatio)}>{formatPacketLoss(currentAvgLossRatio)}</span>
  </div>
</div>
```

替换为：

```tsx
<div className="mb-1 flex items-baseline justify-between">
  <span
    className={`font-semibold text-lg leading-none tabular-nums ${latencyColorClass(currentAvgLatency, {
      failed: isLatencyFailure(currentAvgLossRatio)
    })}`}
  >
    {currentAvgLatency == null || isLatencyFailure(currentAvgLossRatio) ? '—' : currentAvgLatency.toFixed(0)}
    <span className="ml-0.5 font-medium text-[10px] text-muted-foreground">ms</span>
  </span>
  <span className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
    <span className={`size-1.5 rounded-full ${getLossDotBgClass(currentAvgLossRatio)}`} aria-hidden="true" />
    <span>
      {t('card_packet_loss')}{' '}
      <strong className={`font-semibold tabular-nums ${getLossTextClassName(currentAvgLossRatio)}`}>
        {formatPacketLoss(currentAvgLossRatio)}
      </strong>
    </span>
  </span>
</div>
```

- [ ] **Step 5: 预计算 `severityPoints` 并替换柱图**

先在 `latencyPoints` 解构之后（在 `trafficEntry` 行之前）插入预计算：

```ts
const severityPoints = useMemo(
  () => latencyPoints.map((point) => ({ ...point, ...getSeverityBarData(point) })),
  [latencyPoints]
)
```

然后把原来的两个 `<ChartContainer>`（L371-410，包含 latency bar + loss strip）整块替换为：

```tsx
<figure aria-label={t('common:a11y.latency_trend')} className="relative z-10 m-0">
  <ChartContainer className="aspect-auto h-8 w-full" config={LATENCY_CHART_CONFIG}>
    <BarChart
      accessibilityLayer
      barCategoryGap={CHART_BAR_GAP}
      data={severityPoints}
      margin={{ bottom: 0, left: 0, right: 0, top: 0 }}
    >
      <defs>
        <pattern
          id="latency-fail-stripe"
          width="5"
          height="5"
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <rect width="5" height="5" fill="#ef4444" />
          <line x1="0" y1="0" x2="0" y2="5" stroke="rgba(0,0,0,0.25)" strokeWidth="2" />
        </pattern>
      </defs>
      <ChartTooltip content={<NetworkChartTooltip t={t} />} cursor={false} />
      <Bar
        background={{ fill: 'transparent' }}
        dataKey="value"
        isAnimationActive={false}
        shape={(shapeProps) => <SeverityBar {...shapeProps} failPatternId="latency-fail-stripe" />}
      />
    </BarChart>
  </ChartContainer>
</figure>
```

注意：
- 高度从 `h-7` 改成 `h-8`。原本 `h-7 + mt-0.5 + h-1 ≈ 34px`，新版单个 `h-8 = 32px`，整体视觉高度接近。
- Recharts 会给 `shape` 传 `payload`（即数据项），`SeverityBar` 从 `payload.combinedSeverity` 和 `payload.fillColor` 读取。
- **`background={{ fill: 'transparent' }}` 是必需的**。Recharts 只有在 `<Bar>` 上显式设置 `background` 时才会计算并向 `shape` 透传 `background` 矩形几何。失败柱（value=null/0）需要这个 geometry 才能渲染成满格高度。透明填充保证视觉上无额外背景色。
- 不再需要 `<Cell>` map——`shape` 负责每根柱的样式。后续清理 import 时 `Cell` 会被自动移除。

- [ ] **Step 6: 清理未使用 import**

运行：

```bash
cd apps/web && bun x ultracite fix src/components/server/server-card.tsx
```

这会自动移除 `Cell`、`getLatencyBarColor` 等未用导入。

- [ ] **Step 7: 运行 typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: 无错误。

- [ ] **Step 8: 运行 lint**

Run: `cd apps/web && bun x ultracite check`
Expected: 无错误。

- [ ] **Step 9: 提交**

```bash
git add apps/web/src/components/server/server-card.tsx apps/web/src/components/server/severity-bar.tsx
git commit -m "feat(web): compact latency header with unified severity bars"
```

---

## Task 4: 清理 i18n key `card_latency`

**Files:**
- Modify: `apps/web/src/locales/zh/servers.json`
- Modify: `apps/web/src/locales/en/servers.json`

- [ ] **Step 1: 确认 `card_latency` 只在 server-card.tsx 里用过且已被移除**

Run: `cd apps/web && grep -rn "card_latency" src/ --include='*.tsx' --include='*.ts'`
Expected: 无输出（头部重构后不再使用该 key 作为文本 label）。

- [ ] **Step 2: 从 `apps/web/src/locales/zh/servers.json` 删除 `"card_latency": "延迟"` 行**

用 Edit 工具移除对应键值对（注意同步修正前后逗号）。

- [ ] **Step 3: 从 `apps/web/src/locales/en/servers.json` 删除 `"card_latency": "Latency"` 行**

同上。

- [ ] **Step 4: 运行 typecheck 确认 i18n 类型无错**

Run: `cd apps/web && bun run typecheck`
Expected: 无错误。

- [ ] **Step 5: 提交**

```bash
git add apps/web/src/locales/zh/servers.json apps/web/src/locales/en/servers.json
git commit -m "chore(web): drop unused card_latency i18n key"
```

---

## Task 5: 视觉验证（prod-proxy 模式）

**Files:** 无修改

- [ ] **Step 1: 启动 prod-proxy**

Run: `make web-dev-prod`
Expected: Vite 启动成功，终端提示 `➜  Local:   http://localhost:5173/`。

- [ ] **Step 2: 浏览器打开 http://localhost:5173/servers?view=grid**

逐个卡片目视验证：

| 场景 | 预期 |
|------|------|
| 健康服务器（低延迟 + 0 丢包） | 延迟字绿色、丢包圆点绿、所有柱绿色 |
| 有 1-3% 丢包的服务器 | 延迟字可能仍绿、丢包副文/圆点琥珀、部分柱琥珀 |
| 有高丢包（≥5%）的服务器 | 柱体有红色段、丢包字红色 |
| 有完全失败记录 | 柱体有满格 45° 斜线红柱（不只是"高红柱"） |

- [ ] **Step 3: 鼠标悬停有数据的柱上，确认 tooltip 显示每个目标的延迟 + 丢包**

Expected: tooltip 内容和改造前一致（多目标列表）。

- [ ] **Step 4: 切换深色模式**

点右上角主题切换或系统主题。
Expected: 文字、圆点、柱体色在深色背景下仍清晰可辨。

- [ ] **Step 5: 切换到 list 视图验证其他视图无回归**

URL 改 `view=list`，确认列表视图没受影响（这次不改 list 视图）。

- [ ] **Step 6: 关闭 dev server 并提交验证笔记**

按 Ctrl+C 停止 dev server。无需再提交（本任务不改代码）。

---

## Task 6: 最终全量 CI 验证

**Files:** 无修改

- [ ] **Step 1: 运行 web 全量测试**

Run: `cd apps/web && bun run test`
Expected: 121+ 个测试全绿（含本次新增的 severity case）。

- [ ] **Step 2: 运行 lint**

Run: `cd apps/web && bun x ultracite check`
Expected: 无错误。

- [ ] **Step 3: 运行 typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: 无错误。

- [ ] **Step 4: 确认工作区干净**

Run: `git status`
Expected: 无未提交修改。

- [ ] **Step 5: 查看提交历史**

Run: `git log --oneline -5`
Expected: 看到本次 4 个提交（Task 1、Task 2、Task 3、Task 4）按顺序。

---

## 自检清单（编写后已验证）

- [x] Spec 第 1 节（整体结构）→ Task 3 Step 4-5
- [x] Spec 第 2 节（紧凑头部）→ Task 3 Step 4
- [x] Spec 第 3 节（严重度阈值）→ Task 1 实现
- [x] Spec 第 4 节（合并柱颜色函数）→ Task 1
- [x] Spec 第 5 节（失败柱满格 + 斜线）→ Task 2 + Task 3 Step 5
- [x] Spec 第 6 节（圆点独立取色）→ Task 1 (`getLossDotBgClass`) + Task 3 Step 4
- [x] Spec 第 7 节（保留 tooltip + 数据管线）→ Task 3 Step 5（保留 `NetworkChartTooltip`）
- [x] Spec 第 8 节（删除 loss strip + `getLossStripColor` + `LOSS_STRIP_CONFIG`）→ Task 3 Step 2, 5
- [x] Spec 第 9 节（新增 severity 函数 + SeverityBar + pattern）→ Task 1, 2, 3
- [x] 非目标（不改协议/详情页/阈值）→ 计划中无相关改动
