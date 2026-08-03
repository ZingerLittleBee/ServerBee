import { useQueries } from '@tanstack/react-query'
import { lazy, Suspense, useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { ServerMetricRecord } from '@/lib/api-schema'
import type { ServerMetrics } from '@/lib/server-catalog'
import { formatBytes } from '@/lib/utils'
import {
  extractRecordMetric,
  formatChartDateTime,
  formatChartTime,
  isNetworkMetric,
  METRIC_UNITS,
  metricLabel
} from '@/lib/widget-helpers'
import type { MultiLineConfig } from '@/lib/widget-types'

interface MultiLineWidgetProps {
  config: MultiLineConfig
  servers: ServerMetrics[]
  title?: string | null
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'
const REFETCH_INTERVAL = 60_000

const CHART_COLORS = ['var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)', 'var(--chart-5)']

const TARGET_POINTS = 300
const MIN_BUCKET_MS = 60_000

const LazyMetricLinePlot = lazy(() =>
  import('@/components/charts/metric-line-plot').then((module) => ({
    default: module.MetricLinePlot
  }))
)

interface Bucket {
  counts: Map<string, number>
  sums: Map<string, number>
  timestamp: string
}

function accumulateBuckets(
  serverIds: string[],
  queries: { data?: ServerMetricRecord[] }[],
  serverMap: Map<string, ServerMetrics>,
  metric: string,
  bucketMs: number
): Map<number, Bucket> {
  const buckets = new Map<number, Bucket>()

  for (let i = 0; i < serverIds.length; i++) {
    const sid = serverIds[i]
    const records = queries[i]?.data
    if (!records) {
      continue
    }
    const server = serverMap.get(sid)
    for (const record of records) {
      const start = Math.floor(new Date(record.time).getTime() / bucketMs) * bucketMs
      let bucket = buckets.get(start)
      if (!bucket) {
        bucket = { timestamp: new Date(start).toISOString(), sums: new Map(), counts: new Map() }
        buckets.set(start, bucket)
      }
      const value = extractRecordMetric(record, metric, server)
      bucket.sums.set(sid, (bucket.sums.get(sid) ?? 0) + value)
      bucket.counts.set(sid, (bucket.counts.get(sid) ?? 0) + 1)
    }
  }

  return buckets
}

// Raw records are ~1 point/minute/server; over a 24h window with several servers
// that is thousands of SVG points, which makes the chart laggy. Downsample into
// shared time buckets (averaging per server). Shared bucket keys also let the
// tooltip show every VPS at the hovered point.
function buildBucketedRows(
  serverIds: string[],
  queries: { data?: ServerMetricRecord[] }[],
  serverMap: Map<string, ServerMetrics>,
  metric: string,
  hours: number
): Record<string, unknown>[] {
  const bucketMs = Math.max(MIN_BUCKET_MS, Math.ceil((hours * 3600 * 1000) / TARGET_POINTS))
  const buckets = accumulateBuckets(serverIds, queries, serverMap, metric, bucketMs)

  return Array.from(buckets.entries())
    .toSorted(([a], [b]) => a - b)
    .map(([, bucket]) => {
      const row: Record<string, unknown> = { timestamp: bucket.timestamp }
      for (const [sid, sum] of bucket.sums) {
        row[sid] = sum / (bucket.counts.get(sid) ?? 1)
      }
      return row
    })
}

export function MultiLineWidget({ config, servers, title }: MultiLineWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { server_ids = [], metric } = config
  const hours = config.hours ?? DEFAULT_HOURS
  const interval = config.interval ?? DEFAULT_INTERVAL

  // Sliding time window: update `from`/`to` on each refetch cycle
  const [timeRange, setTimeRange] = useState(() => {
    const now = new Date()
    return {
      from: new Date(now.getTime() - hours * 3600 * 1000).toISOString(),
      to: now.toISOString()
    }
  })

  const refreshTimeRange = useCallback(() => {
    const now = new Date()
    setTimeRange({
      from: new Date(now.getTime() - hours * 3600 * 1000).toISOString(),
      to: now.toISOString()
    })
  }, [hours])

  const queries = useQueries({
    queries: server_ids.map((sid) => ({
      queryKey: ['servers', sid, 'records', hours, interval],
      queryFn: () => {
        refreshTimeRange()
        return api.get<ServerMetricRecord[]>(
          `/api/servers/${sid}/records?from=${encodeURIComponent(timeRange.from)}&to=${encodeURIComponent(timeRange.to)}&interval=${encodeURIComponent(interval)}`
        )
      },
      enabled: sid.length > 0,
      refetchInterval: REFETCH_INTERVAL
    }))
  })

  const isLoading = queries.some((q) => q.isLoading)
  const isNetwork = isNetworkMetric(metric)
  const unit = METRIC_UNITS[metric] ?? ''

  const serverMap = useMemo(() => {
    const map = new Map<string, ServerMetrics>()
    for (const s of servers) {
      map.set(s.id, s)
    }
    return map
  }, [servers])

  const chartSeries = useMemo(
    () =>
      server_ids.map((sid, index) => ({
        dataKey: sid,
        label: serverMap.get(sid)?.name ?? sid.slice(0, 8),
        color: CHART_COLORS[index % CHART_COLORS.length] ?? 'var(--chart-1)'
      })),
    [server_ids, serverMap]
  )

  const chartData = useMemo(
    () => buildBucketedRows(server_ids, queries, serverMap, metric, hours),
    [queries, server_ids, metric, serverMap, hours]
  )

  const label = metricLabel(metric, t)

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-40" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  return (
    <div className="flex h-full min-w-0 flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{title ?? t('widgets.multiLine.title', { metric: label })}</h3>
      <div className="min-h-0 min-w-0 flex-1">
        <Suspense fallback={<Skeleton className="h-full min-h-0 w-full" />}>
          <LazyMetricLinePlot
            ariaLabel={title ?? t('widgets.multiLine.title', { metric: label })}
            className="h-full min-h-0 w-full"
            data={chartData}
            formatTime={formatChartTime}
            formatTooltipLabel={formatChartDateTime}
            formatValue={(value) => (isNetwork ? `${formatBytes(value)}/s` : `${value.toFixed(1)}${unit}`)}
            formatYAxisValue={isNetwork ? formatBytes : undefined}
            series={chartSeries}
            timeLabel={t('chart_time')}
            yMarginLeft={isNetwork ? 68 : 52}
          />
        </Suspense>
      </div>
    </div>
  )
}
