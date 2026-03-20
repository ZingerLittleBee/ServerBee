import { useMemo } from 'react'
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'
import { useServerRecords } from '@/hooks/use-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { formatBytes } from '@/lib/utils'
import type { LineChartConfig } from '@/lib/widget-types'

interface LineChartWidgetProps {
  config: LineChartConfig
  servers: ServerMetrics[]
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'

const METRIC_LABELS: Record<string, string> = {
  cpu: 'CPU',
  memory: 'Memory',
  disk: 'Disk',
  load1: 'Load (1m)',
  load5: 'Load (5m)',
  load15: 'Load (15m)',
  net_in: 'Network In',
  net_out: 'Network Out'
}

const METRIC_UNITS: Record<string, string> = {
  cpu: '%',
  memory: '%',
  disk: '%'
}

function isNetworkMetric(metric: string): boolean {
  return metric === 'net_in' || metric === 'net_out'
}

function formatTime(time: string): string {
  const date = new Date(time)
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function LineChartWidget({ config, servers }: LineChartWidgetProps) {
  const { server_id, metric } = config
  const hours = config.hours ?? DEFAULT_HOURS
  const interval = config.interval ?? DEFAULT_INTERVAL

  const { data: records, isLoading } = useServerRecords(server_id, hours, interval)

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return records.map((r) => {
      let value: number
      switch (metric) {
        case 'cpu':
          value = r.cpu
          break
        case 'memory':
          value = server?.mem_total ? (r.mem_used / server.mem_total) * 100 : 0
          break
        case 'disk':
          value = server?.disk_total ? (r.disk_used / server.disk_total) * 100 : 0
          break
        case 'load1':
          value = r.load1
          break
        case 'load5':
          value = r.load5
          break
        case 'load15':
          value = r.load15
          break
        case 'net_in':
          value = r.net_in_speed
          break
        case 'net_out':
          value = r.net_out_speed
          break
        default:
          value = 0
      }
      return { timestamp: r.time, value }
    })
  }, [records, metric, server])

  const label = METRIC_LABELS[metric] ?? metric
  const unit = METRIC_UNITS[metric] ?? ''
  const serverName = server?.name ?? 'Unknown'
  const isNetwork = isNetworkMetric(metric)

  const chartConfig = {
    value: { label, color: 'var(--chart-1)' }
  } satisfies ChartConfig

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-3">
        <h3 className="font-semibold text-sm">{label}</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
      </div>
      <div className="min-h-0 flex-1">
        <ChartContainer className="h-full w-full" config={chartConfig}>
          <AreaChart accessibilityLayer data={chartData}>
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
            <Area
              dataKey="value"
              fill="var(--color-value)"
              fillOpacity={0.1}
              stroke="var(--color-value)"
              strokeWidth={2}
              type="monotone"
            />
          </AreaChart>
        </ChartContainer>
      </div>
    </div>
  )
}
