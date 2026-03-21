import { useQueries } from '@tanstack/react-query'
import { useCallback, useMemo, useState } from 'react'
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from 'recharts'
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent
} from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerMetricRecord } from '@/lib/api-schema'
import { formatBytes } from '@/lib/utils'
import {
  extractRecordMetric,
  formatChartTime,
  isNetworkMetric,
  METRIC_LABELS,
  METRIC_UNITS
} from '@/lib/widget-helpers'
import type { MultiLineConfig } from '@/lib/widget-types'

interface MultiLineWidgetProps {
  config: MultiLineConfig
  servers: ServerMetrics[]
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'
const REFETCH_INTERVAL = 60_000

const CHART_COLORS = ['var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)', 'var(--chart-5)']

export function MultiLineWidget({ config, servers }: MultiLineWidgetProps) {
  const { server_ids, metric } = config
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

  const chartConfig = useMemo(() => {
    const cfg: ChartConfig = {}
    for (let i = 0; i < server_ids.length; i++) {
      const sid = server_ids[i]
      const name = serverMap.get(sid)?.name ?? sid.slice(0, 8)
      cfg[sid] = { label: name, color: CHART_COLORS[i % CHART_COLORS.length] }
    }
    return cfg
  }, [server_ids, serverMap])

  const chartData = useMemo(() => {
    const timeMap = new Map<string, Record<string, unknown>>()

    for (let i = 0; i < server_ids.length; i++) {
      const sid = server_ids[i]
      const records = queries[i]?.data
      if (!records) {
        continue
      }
      const server = serverMap.get(sid)
      for (const record of records) {
        const key = record.time
        let row = timeMap.get(key)
        if (!row) {
          row = { timestamp: key }
          timeMap.set(key, row)
        }
        row[sid] = extractRecordMetric(record, metric, server)
      }
    }

    return [...timeMap.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([, row]) => row)
  }, [queries, server_ids, metric, serverMap])

  const label = METRIC_LABELS[metric] ?? metric

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-40" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{label} Comparison</h3>
      <div className="min-h-0 flex-1">
        <ChartContainer className="h-full w-full" config={chartConfig}>
          <LineChart accessibilityLayer data={chartData}>
            <CartesianGrid vertical={false} />
            <XAxis axisLine={false} dataKey="timestamp" tickFormatter={formatChartTime} tickLine={false} />
            <YAxis
              axisLine={false}
              tickFormatter={isNetwork ? (v: number) => formatBytes(v) : undefined}
              tickLine={false}
              width={isNetwork ? 60 : 45}
            />
            <ChartTooltip
              content={
                <ChartTooltipContent
                  labelFormatter={(l) => formatChartTime(String(l))}
                  valueFormatter={(v) => (isNetwork ? `${formatBytes(v)}/s` : `${Number(v).toFixed(1)}${unit}`)}
                />
              }
            />
            <ChartLegend content={<ChartLegendContent />} />
            {server_ids.map((sid) => (
              <Line
                dataKey={sid}
                dot={false}
                key={sid}
                stroke={`var(--color-${sid})`}
                strokeWidth={2}
                type="monotone"
              />
            ))}
          </LineChart>
        </ChartContainer>
      </div>
    </div>
  )
}
