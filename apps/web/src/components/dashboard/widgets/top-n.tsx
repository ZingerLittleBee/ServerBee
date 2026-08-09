import { useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar } from '@/components/charts/bar'
import { BarChart } from '@/components/charts/bar-chart'
import { BarValueAxis } from '@/components/charts/bar-value-axis'
import { BarYAxis } from '@/components/charts/bar-y-axis'
import { Grid } from '@/components/charts/grid'
import { ChartTooltip } from '@/components/charts/tooltip/chart-tooltip'
import { TooltipContent, type TooltipRow } from '@/components/charts/tooltip/tooltip-content'
import type { ServerMetrics } from '@/lib/server-catalog'
import { formatBytes } from '@/lib/utils'
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
/** Left gutter for category labels (server names). */
const CATEGORY_MARGIN_LEFT = 96

const PERCENT_METRICS = new Set(['cpu', 'memory', 'disk', 'swap'])

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

export function TopNWidget({ config, servers }: TopNWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { metric, sort = 'desc' } = config
  const count = config.count ?? 5
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

  const formatCategory = useCallback((id: string) => nameById.get(id) ?? id, [nameById])
  const formatValueTick = useCallback((value: number) => formatAxisValue(metric, value), [metric])
  const tooltipRows = useCallback(
    (point: Record<string, unknown>): TooltipRow[] => {
      const value = point.value
      if (typeof value !== 'number' || !Number.isFinite(value)) {
        return []
      }
      return [{ color: 'var(--chart-1)', label: metricName, value: formatValue(metric, value) }]
    },
    [metric, metricName]
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
                margin={{ left: CATEGORY_MARGIN_LEFT, right: 16, top: 4, bottom: 28 }}
                maxBarWidth={MAX_BAR_WIDTH}
                orientation="horizontal"
                valueDomain={valueDomain}
                xDataKey="id"
              >
                <Grid horizontal={false} vertical />
                <BarYAxis formatLabel={formatCategory} />
                <BarValueAxis formatValue={formatValueTick} />
                <ChartTooltip
                  content={({ point }) => <TooltipContent rows={tooltipRows(point)} title={tooltipTitle(point)} />}
                  showDatePill={false}
                />
                <Bar dataKey="value" fill="var(--chart-1)" lineCap={BAR_CORNER_RADIUS} />
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
