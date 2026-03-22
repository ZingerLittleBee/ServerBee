import { useMemo } from 'react'
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'
import { useServerRecords } from '@/hooks/use-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { formatBytes } from '@/lib/utils'
import {
  extractRecordMetric,
  formatChartTime,
  isNetworkMetric,
  METRIC_LABELS,
  METRIC_UNITS
} from '@/lib/widget-helpers'
import type { LineChartConfig } from '@/lib/widget-types'

interface LineChartWidgetProps {
  config: LineChartConfig
  servers: ServerMetrics[]
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'

export function LineChartWidget({ config, servers }: LineChartWidgetProps) {
  const server_id = config.server_id ?? ''
  const { metric } = config
  const hours = config.hours ?? DEFAULT_HOURS
  const interval = config.interval ?? DEFAULT_INTERVAL

  const { data: records, isLoading } = useServerRecords(server_id, hours, interval)

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return records.map((r) => ({
      timestamp: r.time,
      value: extractRecordMetric(r, metric, server)
    }))
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
