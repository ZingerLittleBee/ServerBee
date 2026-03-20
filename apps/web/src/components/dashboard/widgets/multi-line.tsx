import { useQueries } from '@tanstack/react-query'
import { useMemo } from 'react'
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
import type { MultiLineConfig } from '@/lib/widget-types'

interface MultiLineWidgetProps {
  config: MultiLineConfig
  servers: ServerMetrics[]
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'

const CHART_COLORS = ['var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)', 'var(--chart-5)']

const METRIC_LABELS: Record<string, string> = {
  cpu: 'CPU',
  memory: 'Memory',
  disk: 'Disk',
  load1: 'Load (1m)',
  net_in: 'Network In',
  net_out: 'Network Out'
}

function isNetworkMetric(metric: string): boolean {
  return metric === 'net_in' || metric === 'net_out'
}

function extractValue(record: ServerMetricRecord, metric: string, server?: ServerMetrics): number {
  switch (metric) {
    case 'cpu':
      return record.cpu
    case 'memory':
      return server?.mem_total ? (record.mem_used / server.mem_total) * 100 : 0
    case 'disk':
      return server?.disk_total ? (record.disk_used / server.disk_total) * 100 : 0
    case 'load1':
      return record.load1
    case 'load5':
      return record.load5
    case 'load15':
      return record.load15
    case 'net_in':
      return record.net_in_speed
    case 'net_out':
      return record.net_out_speed
    default:
      return 0
  }
}

function formatTime(time: string): string {
  const date = new Date(time)
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

const METRIC_UNITS: Record<string, string> = {
  cpu: '%',
  memory: '%',
  disk: '%'
}

export function MultiLineWidget({ config, servers }: MultiLineWidgetProps) {
  const { server_ids, metric } = config
  const hours = config.hours ?? DEFAULT_HOURS
  const interval = config.interval ?? DEFAULT_INTERVAL

  const now = useMemo(() => new Date(), [])
  const from = useMemo(() => new Date(now.getTime() - hours * 3600 * 1000).toISOString(), [now, hours])
  const to = useMemo(() => now.toISOString(), [now])

  const queries = useQueries({
    queries: server_ids.map((sid) => ({
      queryKey: ['servers', sid, 'records', hours, interval],
      queryFn: () =>
        api.get<ServerMetricRecord[]>(
          `/api/servers/${sid}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(interval)}`
        ),
      enabled: sid.length > 0,
      refetchInterval: 60_000
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

  // Build chart config with server names as labels
  const chartConfig = useMemo(() => {
    const cfg: ChartConfig = {}
    for (let i = 0; i < server_ids.length; i++) {
      const sid = server_ids[i]
      const name = serverMap.get(sid)?.name ?? sid.slice(0, 8)
      cfg[sid] = { label: name, color: CHART_COLORS[i % CHART_COLORS.length] }
    }
    return cfg
  }, [server_ids, serverMap])

  // Merge all server data into a single dataset keyed by timestamp
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
        row[sid] = extractValue(record, metric, server)
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
            <XAxis axisLine={false} dataKey="timestamp" tickFormatter={formatTime} tickLine={false} />
            <YAxis
              axisLine={false}
              tickFormatter={isNetwork ? (v: number) => formatBytes(v) : undefined}
              tickLine={false}
              width={isNetwork ? 60 : 45}
            />
            <ChartTooltip
              content={
                <ChartTooltipContent
                  labelFormatter={(l) => formatTime(String(l))}
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
