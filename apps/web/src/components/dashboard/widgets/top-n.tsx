import { useCallback, useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { Bar } from '@/components/charts/bar'
import { BarChart } from '@/components/charts/bar-chart'
import { BarValueAxis } from '@/components/charts/bar-value-axis'
import { useChart, useChartStable } from '@/components/charts/chart-context'
import { Grid } from '@/components/charts/grid'
import { ChartTooltip } from '@/components/charts/tooltip/chart-tooltip'
import { TooltipContent, type TooltipRow } from '@/components/charts/tooltip/tooltip-content'
import type { ServerMetrics } from '@/lib/server-catalog'
import { cn, formatBytes } from '@/lib/utils'
import { extractLiveMetric, metricLabel } from '@/lib/widget-helpers'
import type { TopNConfig } from '@/lib/widget-types'

interface TopNWidgetProps {
  config: TopNConfig
  servers: ServerMetrics[]
}

interface TopNRow {
  id: string
  name: string
  value: number
}

/** Corner radius matching shadcn horizontal bar example (`radius={5}`). */
const BAR_CORNER_RADIUS = 5
/** Cap bar thickness so sparse rankings still look like bars, not blocks. */
const MAX_BAR_WIDTH = 28
/** Vertical space per ranked row (band + breathing room). */
const ROW_HEIGHT_PX = 36
/** Room for value-axis ticks under the plot. */
const CHART_BOTTOM_PAD_PX = 32
/** Plot insets — no left category axis; names live on the bars. */
const PLOT_MARGIN = { left: 8, right: 16, top: 4, bottom: 28 } as const
/** Horizontal padding between bar edge and the name label. */
const LABEL_INSET_PX = 8
/** Bars narrower than this put the name to the right of the fill (outside). */
const MIN_INNER_LABEL_WIDTH_PX = 56

/** Default bar fill when the widget config omits `color`. */
export const DEFAULT_TOP_N_BAR_COLOR = '#8EC5FF'

const PERCENT_METRICS = new Set(['cpu', 'memory', 'disk', 'swap'])
const HEX6_RE = /^#[0-9A-Fa-f]{6}$/
const HEX3_RE = /^#[0-9A-Fa-f]{3}$/

/** Normalize user-entered hex (`#RGB`, `#RRGGBB`, optional leading `#`) to `#RRGGBB`. */
export function normalizeTopNBarColor(value: string | undefined | null): string | null {
  if (value == null) {
    return null
  }
  const trimmed = value.trim()
  if (trimmed.length === 0) {
    return null
  }
  const withHash = trimmed.startsWith('#') ? trimmed : `#${trimmed}`
  if (HEX6_RE.test(withHash)) {
    return withHash.toUpperCase()
  }
  if (HEX3_RE.test(withHash)) {
    const r = withHash[1]
    const g = withHash[2]
    const b = withHash[3]
    return `#${r}${r}${g}${g}${b}${b}`.toUpperCase()
  }
  return null
}

export function resolveTopNBarColor(value: string | undefined | null): string {
  return normalizeTopNBarColor(value) ?? DEFAULT_TOP_N_BAR_COLOR
}

/** Relative luminance of `#RRGGBB` (sRGB). Used to pick label ink on the bar. */
function hexRelativeLuminance(hex: string): number {
  const raw = hex.replace('#', '')
  const channels = [0, 2, 4].map((offset) => {
    const channel = Number.parseInt(raw.slice(offset, offset + 2), 16) / 255
    return channel <= 0.039_28 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2]
}

function labelClassForBar(fillHex: string, inside: boolean): string {
  if (!inside) {
    return 'text-foreground'
  }
  // better-colors: light fill (L-ish via WCAG luminance) → dark text.
  if (hexRelativeLuminance(fillHex) > 0.45) {
    return 'text-zinc-950'
  }
  return 'text-white [text-shadow:0_1px_1px_rgba(0,0,0,0.45)]'
}

function formatValue(metric: string, value: number): string {
  if (metric === 'bandwidth' || metric === 'network' || metric === 'disk_io') {
    return `${formatBytes(value)}/s`
  }
  if (PERCENT_METRICS.has(metric)) {
    return `${value.toFixed(1)}%`
  }
  return value.toFixed(1)
}

function formatAxisValue(metric: string, value: number): string {
  if (metric === 'bandwidth' || metric === 'network' || metric === 'disk_io') {
    return formatBytes(value)
  }
  if (PERCENT_METRICS.has(metric)) {
    return `${value.toFixed(0)}%`
  }
  return value.toFixed(0)
}

function chartHeightForCount(count: number): number {
  return Math.max(ROW_HEIGHT_PX, count * ROW_HEIGHT_PX) + CHART_BOTTOM_PAD_PX
}

interface TopNBarLabelsProps {
  fillColor: string
  getName: (id: string) => string
}

/**
 * Name labels painted on (or just past) each horizontal bar. Lives as a
 * post-overlay child of `BarChart` so it can read band geometry from context
 * without reserving a separate category axis.
 */
function TopNBarLabels({ fillColor, getName }: TopNBarLabelsProps) {
  const { containerRef } = useChartStable()
  const [mounted, setMounted] = useState(false)
  const { barScale, bandWidth, barXAccessor, data, margin, yScale, hoveredBarIndex } = useChart()

  useEffect(() => {
    setMounted(true)
  }, [])

  const container = containerRef.current
  const labels = useMemo(() => {
    if (!(barScale && bandWidth && barXAccessor)) {
      return []
    }
    const zeroX = yScale(0) ?? 0
    return data.map((point, index) => {
      const id = barXAccessor(point)
      const rawValue = point.value
      const value = typeof rawValue === 'number' && Number.isFinite(rawValue) ? rawValue : 0
      const barWidthPx = Math.max(0, (yScale(value) ?? 0) - zeroX)
      const inside = barWidthPx >= MIN_INNER_LABEL_WIDTH_PX
      const bandY = barScale(id) ?? 0
      return {
        barWidthPx,
        id,
        index,
        inside,
        name: getName(id),
        y: bandY + margin.top,
        bandHeight: bandWidth
      }
    })
  }, [barScale, bandWidth, barXAccessor, data, getName, margin.top, yScale])

  if (!(mounted && container)) {
    return null
  }

  return createPortal(
    <div className="pointer-events-none absolute inset-0" data-testid="top-n-bar-labels">
      {labels.map((item) => {
        const isHovered = hoveredBarIndex === item.index
        return (
          <div
            className="absolute flex items-center"
            key={item.id}
            style={{
              height: item.bandHeight,
              left: item.inside ? margin.left + LABEL_INSET_PX : margin.left + item.barWidthPx + LABEL_INSET_PX,
              maxWidth: item.inside
                ? Math.max(0, item.barWidthPx - LABEL_INSET_PX * 2)
                : Math.max(
                    0,
                    (container.clientWidth || 0) - margin.left - item.barWidthPx - LABEL_INSET_PX - margin.right
                  ),
              top: item.y
            }}
          >
            <span
              className={cn(
                'truncate font-medium text-xs transition-opacity duration-150',
                labelClassForBar(fillColor, item.inside),
                isHovered ? 'opacity-100' : 'opacity-90'
              )}
            >
              {item.name}
            </span>
          </div>
        )
      })}
    </div>,
    container
  )
}
// Render above bars / interaction overlay so names stay visible while hovering.
;(TopNBarLabels as typeof TopNBarLabels & { __isPostOverlay?: boolean }).__isPostOverlay = true
TopNBarLabels.displayName = 'TopNBarLabels'

export function TopNWidget({ config, servers }: TopNWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { metric, sort = 'desc' } = config
  const count = config.count ?? 5
  const barColor = resolveTopNBarColor(config.color)
  const metricName = metricLabel(metric, t)
  const title = t('widgets.topN.title', { metric: metricName })

  const ranked = useMemo<TopNRow[]>(() => {
    const online = servers.filter((s) => s.online)
    const withMetric = online.map((s) => ({
      id: s.id,
      name: s.name,
      value: extractLiveMetric(s, metric)
    }))

    withMetric.sort((a, b) => (sort === 'desc' ? b.value - a.value : a.value - b.value))

    return withMetric.slice(0, count)
  }, [servers, metric, count, sort])

  const nameById = useMemo(() => new Map(ranked.map((row) => [row.id, row.name])), [ranked])

  const chartData = useMemo(
    // Band scale range is [0, height] (SVG y grows downward), so the first
    // domain entry paints at the top — keep rank order as-is (#1 first).
    () =>
      ranked.map((row) => ({
        id: row.id,
        name: row.name,
        value: row.value
      })),
    [ranked]
  )

  const valueDomain = useMemo((): [number, number] | undefined => {
    if (PERCENT_METRICS.has(metric)) {
      return [0, 100]
    }
    return undefined
  }, [metric])

  const getName = useCallback((id: string) => nameById.get(id) ?? id, [nameById])
  const formatValueTick = useCallback((value: number) => formatAxisValue(metric, value), [metric])
  const tooltipRows = useCallback(
    (point: Record<string, unknown>): TooltipRow[] => {
      const value = point.value
      if (typeof value !== 'number' || !Number.isFinite(value)) {
        return []
      }
      return [{ color: barColor, label: metricName, value: formatValue(metric, value) }]
    },
    [barColor, metric, metricName]
  )
  const tooltipTitle = useCallback(
    (point: Record<string, unknown>) => {
      const id = String(point.id ?? '')
      return nameById.get(id) ?? id
    },
    [nameById]
  )

  return (
    <div className="flex h-full flex-col justify-center rounded-lg border bg-card">
      {/* data-measure: natural content height (incl. padding), measured by the
          grid to size the cell. Independent of the (h-full) card height. */}
      <div className="flex flex-col gap-3 p-4" data-measure>
        <h3 className="font-semibold text-sm">{title}</h3>
        {ranked.length === 0 ? (
          <div className="flex items-center justify-center py-4 text-muted-foreground text-xs">
            {t('widgets.topN.empty.noServers')}
          </div>
        ) : (
          <figure aria-label={title} className="w-full min-w-0" style={{ height: chartHeightForCount(ranked.length) }}>
            <div aria-hidden="true" className="h-full min-h-0 w-full min-w-0" data-testid="top-n-bar-chart">
              <BarChart
                animationDuration={500}
                aspectRatio=""
                className="h-full min-h-0 w-full min-w-0"
                data={chartData}
                margin={PLOT_MARGIN}
                maxBarWidth={MAX_BAR_WIDTH}
                orientation="horizontal"
                valueDomain={valueDomain}
                xDataKey="id"
              >
                <Grid horizontal={false} vertical />
                <BarValueAxis formatValue={formatValueTick} />
                <ChartTooltip
                  content={({ point }) => <TooltipContent rows={tooltipRows(point)} title={tooltipTitle(point)} />}
                  showDatePill={false}
                />
                <Bar dataKey="value" fill={barColor} lineCap={BAR_CORNER_RADIUS} />
                <TopNBarLabels fillColor={barColor} getName={getName} />
              </BarChart>
            </div>

            <table className="sr-only">
              <caption>{title}</caption>
              <thead>
                <tr>
                  <th scope="col">{t('common.labels.server')}</th>
                  <th scope="col">{metricName}</th>
                </tr>
              </thead>
              <tbody>
                {ranked.map((row) => (
                  <tr key={row.id}>
                    <td>{row.name}</td>
                    <td>{formatValue(metric, row.value)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </figure>
        )}
      </div>
    </div>
  )
}
