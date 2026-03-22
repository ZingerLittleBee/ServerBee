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
import { useServerRecords } from '@/hooks/use-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { buildMergedDiskIoSeries } from '@/lib/disk-io'
import { formatSpeed } from '@/lib/utils'
import { formatChartTime } from '@/lib/widget-helpers'
import type { DiskIoConfig } from '@/lib/widget-types'

interface DiskIoWidgetProps {
  config: DiskIoConfig
  servers: ServerMetrics[]
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'

const chartConfig = {
  read_bytes_per_sec: { label: 'Read', color: 'var(--chart-1)' },
  write_bytes_per_sec: { label: 'Write', color: 'var(--chart-2)' }
} satisfies ChartConfig

export function DiskIoWidget({ config, servers }: DiskIoWidgetProps) {
  const server_id = config.server_id ?? ''
  const hours = config.hours ?? DEFAULT_HOURS
  const interval = config.interval ?? DEFAULT_INTERVAL

  const { data: records, isLoading } = useServerRecords(server_id, hours, interval)

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return buildMergedDiskIoSeries(records)
  }, [records])

  const serverName = server?.name ?? 'Unknown'

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  if (chartData.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">Disk I/O</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          No disk I/O data available
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-3">
        <h3 className="font-semibold text-sm">Disk I/O</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
      </div>
      <div className="min-h-0 flex-1">
        <ChartContainer className="h-full w-full" config={chartConfig}>
          <LineChart accessibilityLayer data={chartData}>
            <CartesianGrid vertical={false} />
            <XAxis axisLine={false} dataKey="timestamp" tickFormatter={formatChartTime} tickLine={false} />
            <YAxis axisLine={false} tickFormatter={formatSpeed} tickLine={false} width={70} />
            <ChartTooltip
              content={
                <ChartTooltipContent
                  labelFormatter={(l) => formatChartTime(String(l))}
                  valueFormatter={(v) => formatSpeed(v)}
                />
              }
            />
            <ChartLegend content={<ChartLegendContent />} />
            <Line
              dataKey="read_bytes_per_sec"
              dot={false}
              stroke="var(--color-read_bytes_per_sec)"
              strokeWidth={2}
              type="monotone"
            />
            <Line
              dataKey="write_bytes_per_sec"
              dot={false}
              stroke="var(--color-write_bytes_per_sec)"
              strokeWidth={2}
              type="monotone"
            />
          </LineChart>
        </ChartContainer>
      </div>
    </div>
  )
}
