import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetHeader, WidgetMessage } from '@/components/dashboard/widget-chrome'
import type { ServerMetrics } from '@/lib/server-catalog'
import { getUtilizationTextColor } from '@/lib/utilization-colors'
import { cn, formatBytes } from '@/lib/utils'
import { normalizeWidgetColor } from '@/lib/widget-color'
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

const PERCENT_METRICS = new Set(['cpu', 'memory', 'disk', 'swap'])
/** Keeps near-zero readings visible as a sliver instead of an empty row. */
const MIN_FILL_PERCENT = 2
const DEFAULT_ACCENT = 'var(--chart-1)'

function formatValue(metric: string, value: number): string {
  if (metric === 'bandwidth' || metric === 'network' || metric === 'disk_io') {
    return `${formatBytes(value)}/s`
  }
  if (PERCENT_METRICS.has(metric)) {
    return `${value.toFixed(1)}%`
  }
  return value.toFixed(1)
}

/** Row fill: the leader gets the accent tint, the rest a neutral track. */
function rowFill(index: number, accent: string, hasCustomColor: boolean): string {
  if (index === 0) {
    return `color-mix(in oklab, ${accent} 20%, transparent)`
  }
  return hasCustomColor ? `color-mix(in oklab, ${accent} 10%, transparent)` : 'var(--muted)'
}

export function TopNWidget({ config, servers }: TopNWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { metric, sort = 'desc' } = config
  const count = config.count ?? 5
  const customColor = normalizeWidgetColor(config.color)
  const accent = customColor ?? DEFAULT_ACCENT
  const metricName = metricLabel(metric, t)
  const title = t('widgets.topN.title', { metric: metricName })
  const isPercent = PERCENT_METRICS.has(metric)

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

  // Percent metrics fill against 100%; open-ended metrics against the largest row.
  const scaleMax = useMemo(() => {
    if (isPercent) {
      return 100
    }
    return Math.max(0, ...ranked.map((row) => row.value))
  }, [isPercent, ranked])

  return (
    <DashboardCard className="flex h-full flex-col justify-center bg-card">
      {/* data-measure: natural content height (incl. padding), measured by the
          grid to size the cell. Independent of the (h-full) card height. */}
      <div className="flex flex-col gap-3 p-5" data-measure>
        <WidgetHeader title={title} />
        {ranked.length === 0 ? (
          <WidgetMessage className="py-4" title={t('widgets.topN.empty.noServers')} />
        ) : (
          <ol aria-label={title} className="flex flex-col gap-1.5" data-testid="top-n-rows">
            {ranked.map((row, index) => {
              const share = scaleMax > 0 ? (row.value / scaleMax) * 100 : 0
              const fillPercent = Math.min(100, Math.max(MIN_FILL_PERCENT, share))
              return (
                <li
                  className="relative flex h-9 items-center gap-3 overflow-hidden rounded-[10px] px-3"
                  data-testid="top-n-row"
                  key={row.id}
                >
                  <span
                    aria-hidden="true"
                    className="absolute inset-y-0 left-0 rounded-[10px]"
                    data-testid="top-n-fill"
                    style={{ width: `${fillPercent}%`, backgroundColor: rowFill(index, accent, customColor !== null) }}
                  />
                  <span className="relative min-w-0 flex-1 truncate font-medium text-sm">{row.name}</span>
                  <span
                    className={cn(
                      'relative shrink-0 font-semibold text-sm tabular-nums',
                      isPercent ? getUtilizationTextColor(row.value) : 'text-foreground'
                    )}
                  >
                    {formatValue(metric, row.value)}
                  </span>
                </li>
              )
            })}
          </ol>
        )}
      </div>
    </DashboardCard>
  )
}
