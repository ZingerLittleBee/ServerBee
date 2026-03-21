import { useMemo } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn, formatBytes } from '@/lib/utils'
import { extractLiveMetric, METRIC_LABELS } from '@/lib/widget-helpers'
import type { TopNConfig } from '@/lib/widget-types'

interface TopNWidgetProps {
  config: TopNConfig
  servers: ServerMetrics[]
}

function formatValue(metric: string, value: number): string {
  if (metric === 'bandwidth') {
    return `${formatBytes(value)}/s`
  }
  return `${value.toFixed(1)}%`
}

function getBarColor(rank: number): string {
  const colors = ['bg-chart-1', 'bg-chart-2', 'bg-chart-3', 'bg-chart-4', 'bg-chart-5']
  return colors[rank % colors.length]
}

const TOP_N_LABELS: Record<string, string> = {
  cpu: 'Top CPU',
  memory: 'Top Memory',
  disk: 'Top Disk',
  bandwidth: 'Top Bandwidth'
}

export function TopNWidget({ config, servers }: TopNWidgetProps) {
  const { metric, sort = 'desc' } = config
  const count = config.count ?? 5

  const ranked = useMemo(() => {
    const online = servers.filter((s) => s.online)
    const withMetric = online.map((s) => ({
      id: s.id,
      name: s.name,
      value: extractLiveMetric(s, metric)
    }))

    withMetric.sort((a, b) => (sort === 'desc' ? b.value - a.value : a.value - b.value))

    return withMetric.slice(0, count)
  }, [servers, metric, count, sort])

  const maxValue = useMemo(() => {
    if (metric === 'bandwidth') {
      return ranked.length > 0 ? Math.max(...ranked.map((r) => r.value), 1) : 1
    }
    return 100
  }, [ranked, metric])

  const title = TOP_N_LABELS[metric] ?? `Top ${METRIC_LABELS[metric] ?? metric}`

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{title}</h3>
      <div className="flex flex-1 flex-col gap-2 overflow-auto">
        {ranked.map((item, index) => {
          const pct = maxValue > 0 ? (item.value / maxValue) * 100 : 0
          return (
            <div className="space-y-1" key={item.id}>
              <div className="flex items-center justify-between text-xs">
                <span className="flex items-center gap-1.5 truncate text-muted-foreground">
                  <span className="font-medium text-foreground">{index + 1}.</span>
                  {item.name}
                </span>
                <span className="ml-2 shrink-0 font-medium">{formatValue(metric, item.value)}</span>
              </div>
              <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                <div
                  className={cn('h-full rounded-full transition-all', getBarColor(index))}
                  style={{ width: `${Math.min(100, pct)}%` }}
                />
              </div>
            </div>
          )
        })}
        {ranked.length === 0 && (
          <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">No online servers</div>
        )}
      </div>
    </div>
  )
}
